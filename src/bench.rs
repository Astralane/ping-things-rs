use crate::config::{convert_to_ws, PingThingsArgs};
use crate::tx_sender::{RpcTxSender, TxMetrics};
use futures::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_api::config::RpcSignatureSubscribeConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::EncodableKey;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct Bench {
    config: PingThingsArgs,
    tx_subscribe_sender: tokio::sync::mpsc::Sender<TxMetrics>,
    cancel: CancellationToken,
    rpcs: Vec<RpcTxSender>,
    hdl: tokio::task::JoinHandle<()>,
}

impl Bench {
    pub fn new(config: PingThingsArgs, cancellation_token: CancellationToken) -> Self {
        let (tx_subscribe_sender, tx_subscribe_receiver) = tokio::sync::mpsc::channel(100);

        let rpcs = config
            .rpc
            .clone()
            .into_iter()
            .map(|(name, rpc)| {
                let keyring = Keypair::read_from_file(config.keypair_dir.clone())
                    .expect("cannot read keypair");
                RpcTxSender::new(
                    name,
                    rpc.url,
                    keyring,
                    config.compute_unit_limit,
                    config.compute_unit_price,
                    config.enable_priority_fee,
                )
            })
            .collect::<Vec<RpcTxSender>>();

        let rpc_names = rpcs
            .iter()
            .map(|rpc| rpc.name.clone())
            .collect::<Vec<String>>();

        let recv_loop_handle = tokio::spawn(Bench::transaction_save_loop(
            tx_subscribe_receiver,
            cancellation_token.clone(),
            rpc_names,
        ));

        Bench {
            config,
            tx_subscribe_sender,
            rpcs,
            cancel: cancellation_token,
            hdl: recv_loop_handle,
        }
    }

    pub async fn transaction_save_loop(
        mut recv: tokio::sync::mpsc::Receiver<TxMetrics>,
        cancel: CancellationToken,
        rpc_names: Vec<String>,
    ) {
        //hash map of rpc name to csv file
        let mut files_map = rpc_names
            .iter()
            .map(|rpc_name| {
                let file_name = format!("{}.csv", rpc_name);
                //delete old files
                std::fs::remove_file(&file_name).ok();
                let file = std::fs::File::create(file_name).expect("cannot create file");
                let writer = csv::Writer::from_writer(file);
                (rpc_name.clone(), writer)
            })
            .collect::<HashMap<_, _>>();
        loop {
            if recv.is_closed() || cancel.is_cancelled() {
                info!("exiting transaction save loop");
                //close all files
                for (_, writer) in files_map.iter_mut() {
                    writer.flush().expect("cannot flush writer");
                }
                break;
            }
            let data = recv.recv().await;
            if let Some(data) = data {
                //save to csv
                let writer = files_map.get_mut(&data.rpc_name).unwrap();
                writer
                    .serialize(&data)
                    .expect(format!("Failed to write to CSV for {:?}", data).as_str());
            }
        }
    }

    pub async fn send_and_confirm_transaction(
        tx_index: u32,
        rpc_sender: RpcTxSender,
        recent_blockhash: Hash,
        slot_sent: u64,
        tx_save_sender: tokio::sync::mpsc::Sender<TxMetrics>,
        rpc_name: String,
        read_rpc_ws: Arc<PubsubClient>,
    ) {
        let start = tokio::time::Instant::now();
        let result = rpc_sender
            .send_transaction(tx_index, recent_blockhash)
            .await;
        match result {
            Ok(signature) => {
                //subscribe to signature
                if let Ok((mut stream, unsub)) = read_rpc_ws
                    .signature_subscribe(
                        &signature,
                        Some(RpcSignatureSubscribeConfig {
                            commitment: Some(CommitmentConfig::processed()),
                            enable_received_notification: Some(false),
                        }),
                    )
                    .await
                {
                    // timeout 60 seconds
                    let timeout_result =
                        tokio::time::timeout(tokio::time::Duration::from_secs(10), stream.next())
                            .await;
                    if let Ok(Some(s)) = timeout_result {
                        tx_save_sender
                            .send(TxMetrics {
                                success: true,
                                elapsed: Some(start.elapsed().as_millis() as u64),
                                slot_sent,
                                slot_landed: Some(s.context.slot),
                                slot_latency: Some(s.context.slot - slot_sent),
                                rpc_name,
                                index: tx_index,
                                signature: signature.to_string(),
                            })
                            .await
                            .expect("cannot send to file saver loop");
                    } else {
                        // log error
                        warn!("no response from signature subscription {:?}", signature);
                        tx_save_sender
                            .send(TxMetrics {
                                success: false,
                                elapsed: None,
                                slot_sent,
                                slot_landed: None,
                                slot_latency: None,
                                rpc_name,
                                index: tx_index,
                                signature: signature.to_string(),
                            })
                            .await
                            .expect("failed to send to receiver");
                    }
                    unsub().await;
                }
            }
            Err(e) => {
                // log error
                error!("error in sending transaction: {:?}", e);
            }
        }
    }

    pub async fn start(self, curr_slot: Arc<AtomicU64>, blk_hash: Arc<RwLock<Option<Hash>>>) {
        let cancel = self.cancel.clone();
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("transaction save loop exited");
            }
            _ = self.start_inner(curr_slot, blk_hash) => {}
        }
    }
    async fn start_inner(self, curr_slot: Arc<AtomicU64>, blk_hash: Arc<RwLock<Option<Hash>>>) {
        info!("starting bench");
        let config = self.config;
        let mut tx_handles = Vec::new();
        info!(
            "connecting to read rpc {:?}",
            convert_to_ws(config.rpc_for_read.clone())
        );
        let read_rpc_ws = Arc::new(
            PubsubClient::new(&convert_to_ws(config.rpc_for_read))
                .await
                .expect("cannot connect to websocket rpc"),
        );

        //wait for blockhash to be updated
        while blk_hash.read().unwrap().is_none() {
            info!("waiting for blockhash to be updated");
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        'outer: for i in 1..=config.runs {
            info!("starting run {}", i);
            for j in 1..=config.txns_per_run {
                // send transaction and save to all rpc
                for rpc in &self.rpcs {
                    if self.cancel.is_cancelled() {
                        info!("cancellation signal received, exiting transaction loop");
                        break 'outer;
                    };
                    let recent_blockhash = {
                        let blk_hash = blk_hash.read().unwrap();
                        (*blk_hash).unwrap()
                    };
                    let tx_save_sender = self.tx_subscribe_sender.clone();
                    let slot_sent = curr_slot.load(Ordering::Relaxed);
                    let rpc_name = rpc.name.clone();
                    let read_rpc_ws = read_rpc_ws.clone();
                    let rpc_sender = rpc.clone();
                    let hdl = tokio::spawn(async move {
                        //unique index based on progress of both loop
                        let index = (i - 1) * config.txns_per_run + j;
                        Self::send_and_confirm_transaction(
                            index,
                            rpc_sender,
                            recent_blockhash,
                            slot_sent,
                            tx_save_sender,
                            rpc_name,
                            read_rpc_ws,
                        )
                        .await;
                    });
                    tx_handles.push(hdl);
                }
            }
            //run delay if not last loop
            if i != config.runs {
                info!("taking a breather...");
                tokio::time::sleep(tokio::time::Duration::from_secs(config.txn_delay as u64)).await;
            }
        }
        info!("waiting for transactions to complete...");
        // wait for all transactions to complete
        for hdl in tx_handles {
            hdl.await.unwrap();
        }
        info!("bench complete!")
    }
}
