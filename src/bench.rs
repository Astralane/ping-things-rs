use crate::config::PingThingsArgs;
use crate::tx_senders::constants::JITO_RPC_URL;
use crate::tx_senders::jito::JitoBundleStatusResponse;
use crate::tx_senders::solana_rpc::TxMetrics;
use crate::tx_senders::transaction::TransactionConfig;
use crate::tx_senders::{create_tx_sender, TxResult, TxSender};
use anyhow::anyhow;
use futures::StreamExt;
use log::debug;
use reqwest::Client;
use serde_json::json;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcSignatureSubscribeConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct Bench {
    config: PingThingsArgs,
    tx_subscribe_sender: tokio::sync::mpsc::Sender<TxMetrics>,
    cancel: CancellationToken,
    rpcs: Vec<Arc<dyn TxSender>>,
    client: Client,
}

impl Bench {
    pub fn new(config: PingThingsArgs, cancellation_token: CancellationToken) -> Self {
        let (tx_subscribe_sender, tx_subscribe_receiver) = tokio::sync::mpsc::channel(100);
        let tx_config: TransactionConfig = config.clone().into();
        let client = Client::new();
        let rpcs = config
            .rpc
            .clone()
            .into_iter()
            .map(|(name, rpc)| create_tx_sender(name, rpc, tx_config.clone(), client.clone()))
            .collect::<Vec<Arc<dyn TxSender>>>();

        let rpc_names = rpcs.iter().map(|rpc| rpc.name()).collect::<Vec<String>>();

        let _recv_loop_handle = tokio::spawn(Bench::transaction_save_loop(
            tx_subscribe_receiver,
            cancellation_token.clone(),
            rpc_names,
        ));

        Bench {
            config,
            tx_subscribe_sender,
            rpcs,
            cancel: cancellation_token,
            client,
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
        rpc_sender: Arc<dyn TxSender>,
        recent_blockhash: Hash,
        slot_sent: u64,
        tx_save_sender: mpsc::Sender<TxMetrics>,
        rpc_name: String,
        http_client: Arc<RpcClient>,
        client: Client,
    ) -> anyhow::Result<()> {
        let start = tokio::time::Instant::now();
        let tx_result = rpc_sender
            .send_transaction(tx_index, recent_blockhash)
            .await?;

        let now = tokio::time::Instant::now();
        let subscription_result = match tx_result.clone() {
            TxResult::Signature(signature) => {
                Self::confirm_transaction(signature, http_client).await
            }
            TxResult::BundleID(id) => unreachable!(),
        };

        if let Ok(slot_landed) = subscription_result {
            let latency = slot_landed.saturating_sub(slot_sent);
            tx_save_sender
                .send(TxMetrics {
                    success: true,
                    elapsed: Some(start.elapsed().as_millis() as u64),
                    slot_sent,
                    slot_landed: Some(slot_landed),
                    slot_latency: Some(latency),
                    rpc_name,
                    index: tx_index,
                    signature: tx_result.into(),
                })
                .await
                .expect("cannot send to file saver loop");
        } else {
            // log error
            warn!(
                "waited {:}s, no response from signature subscription {:?} ",
                now.elapsed().as_secs(),
                tx_result
            );
            tx_save_sender
                .send(TxMetrics {
                    success: false,
                    elapsed: None,
                    slot_sent,
                    slot_landed: None,
                    slot_latency: None,
                    rpc_name,
                    index: tx_index,
                    signature: tx_result.into(),
                })
                .await
                .expect("failed to send to receiver");
        }
        Ok(())
    }

    async fn confirm_transaction(
        signature: Signature,
        client: Arc<RpcClient>,
    ) -> anyhow::Result<u64> {
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            Self::confirm_signature_with_retires(&signature, client),
        )
        .await?;
        result
    }

    async fn confirm_signature_with_retires(
        signature: &Signature,
        client: Arc<RpcClient>,
    ) -> anyhow::Result<u64> {
        let mut max_retries = 10;
        while max_retries > 0 {
            let resp = client
                .get_transaction(signature, UiTransactionEncoding::Base64)
                .await;
            if let Ok(tx) = resp {
                return Ok(tx.slot);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(anyhow!("failed to confirm signature {:?}", signature))
    }
    async fn confirm_bundle(bundle_id: String, client: Client) -> anyhow::Result<u64> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBundleStatuses",
            "params": [[bundle_id.clone()]]
        });
        let confirm_with_retry = async move {
            let mut max_retries = 10;
            while max_retries > 0 {
                match client.post(JITO_RPC_URL).json(&request).send().await {
                    Ok(response) => {
                        let status = response.status();
                        let body = response.text().await?;
                        let parsed_resp = serde_json::from_str::<JitoBundleStatusResponse>(&body);
                        match (parsed_resp, status.is_success()) {
                            (Ok(bundle_responses), true) => {
                                if bundle_responses.result.value.is_empty() {
                                    debug!("empty bundle response: {}", body);
                                    continue;
                                }
                                let slot = bundle_responses.result.value[0].slot;
                                return Ok(slot);
                            }
                            (_, false) => {
                                debug!("failed to confirm bundle: {}", body);
                            }
                            (Err(e), _) => {
                                debug!("failed to parse confirm bundle response {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("failed to confirm bundle {:?}", e);
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                max_retries -= 1;
            }
            Err(anyhow!("failed to confirm bundle {:?}", bundle_id))
        };

        let timeout_result = timeout(Duration::from_secs(60), confirm_with_retry).await;
        let result = timeout_result??;
        Ok(result)
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
        info!("connecting to http rpc {:?}", config.http_rpc.clone());
        let http_rpc = Arc::new(RpcClient::new_with_commitment(
            config.http_rpc.clone(),
            CommitmentConfig::processed(),
        ));

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
                    let rpc_name = rpc.name();
                    let rpc_sender = rpc.clone();
                    let client = self.client.clone();
                    let http_rpc = http_rpc.clone();
                    let hdl = tokio::spawn(async move {
                        //unique index based on progress of both loop
                        let index = (i - 1) * config.txns_per_run + j;
                        if let Err(e) = Self::send_and_confirm_transaction(
                            index,
                            rpc_sender,
                            recent_blockhash,
                            slot_sent,
                            tx_save_sender,
                            rpc_name,
                            http_rpc,
                            client,
                        )
                        .await
                        {
                            error!("error in send_and_confirm_transaction {:?}", e);
                        }
                    });
                    tx_handles.push(hdl);
                }
            }
            //run delay if not last loop
            if i != config.runs {
                info!("taking a breather...");
                tokio::time::sleep(Duration::from_secs(config.txn_delay as u64)).await;
            }
        }
        info!("waiting for transactions to complete...");
        // wait for all transactions to complete
        for hdl in tx_handles {
            hdl.await.unwrap();
        }
        info!("bench complete!")
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
    }
}
