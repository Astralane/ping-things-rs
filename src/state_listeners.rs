use crate::config::{convert_to_http, convert_to_ws};
use futures::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::response::SlotUpdate;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct ChainListener {
    pub current_slot: Arc<AtomicU64>,
    pub recent_blockhash: Arc<RwLock<Option<Hash>>>,
    pub hdl: tokio::task::JoinHandle<()>,
}

impl ChainListener {
    pub fn new(url: String, cancellation_token: CancellationToken) -> Self {
        let current_slot = Arc::new(AtomicU64::from(0u64));
        let recent_blockhash = Arc::new(RwLock::new(None));
        Self {
            hdl: tokio::spawn(Self::listen_to_updates(
                url.clone(),
                current_slot.clone(),
                recent_blockhash.clone(),
                cancellation_token.clone(),
            )),
            current_slot,
            recent_blockhash,
        }
    }

    pub async fn listen_to_updates(
        url: String,
        current_slot: Arc<AtomicU64>,
        recent_blockhash: Arc<RwLock<Option<Hash>>>,
        cancel: CancellationToken,
    ) {
        let http_client = Arc::new(RpcClient::new_with_commitment(
            convert_to_http(url.clone()),
            CommitmentConfig::finalized(),
        ));
        let ws_client = PubsubClient::new(&convert_to_ws(url))
            .await
            .expect("Failed to connect to websocket");
        let slot_hdl = tokio::spawn(Self::listen_to_slot_updates(
            current_slot.clone(),
            ws_client,
            cancel.clone(),
        ));
        Self::listen_to_blockhash_updates(recent_blockhash.clone(), http_client.clone(), cancel)
            .await;
        let _ = slot_hdl.await;
        info!("exit chain listener")
    }

    async fn listen_to_slot_updates(
        current_slot: Arc<AtomicU64>,
        ws_client: PubsubClient,
        cancel: CancellationToken,
    ) {
        let subscription = ws_client.slot_updates_subscribe().await.unwrap();
        let (mut stream, unsub) = subscription;
        while let Some(slot) = tokio::select! {
            _ = cancel.cancelled() => {
                None
            }
            slot = stream.next() => slot,
        } {
            if let SlotUpdate::FirstShredReceived { slot, .. } = slot {
                current_slot.store(slot, Ordering::Relaxed);
            }
            if let SlotUpdate::Completed { slot, .. } = slot {
                current_slot.store(slot + 1, Ordering::Relaxed);
            }
        }
        drop(stream);
        unsub().await;
        //let _ = ws_client.shutdown().await;
    }

    async fn listen_to_blockhash_updates(
        block_hash: Arc<RwLock<Option<Hash>>>,
        http_client: Arc<RpcClient>,
        cancellation_token: CancellationToken,
    ) {
        let update_loop = async move {
            loop {
                let blockhash_res = http_client.get_latest_blockhash().await;
                if let Ok(blockhash) = blockhash_res {
                    let mut lock = block_hash.write().unwrap();
                    *lock = Some(blockhash);
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        };
        tokio::select! {
              _ = cancellation_token.cancelled() => {}
            _ = update_loop => {}
        }
    }
}
