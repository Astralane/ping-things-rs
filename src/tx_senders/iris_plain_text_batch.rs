use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use tokio::time::Instant;
use tracing::info;

pub struct IrisBatchTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl IrisBatchTxSender {
    pub fn new(
        name: String,
        url: String,
        auth: String,
        tx_config: TransactionConfig,
        client: Client,
    ) -> Self {
        Self {
            url,
            auth,
            name,
            tx_config,
            client,
        }
    }
}

#[async_trait]
impl TxSender for IrisBatchTxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_batch(&self) -> bool {
        true
    }

    /// Single-tx fallback: sends a batch of 1.
    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let results = self.send_batch(&[(index, recent_blockhash)]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("sendBatch returned empty result array"))
    }

    /// Send multiple transactions in a single sendBatch JSON-RPC call.
    async fn send_batch(&self, indices: &[(u32, Hash)]) -> anyhow::Result<Vec<TxResult>> {
        // Build and encode all transactions
        let mut expected_sigs = Vec::with_capacity(indices.len());
        let encoded_txns: Vec<String> = indices
            .iter()
            .map(|&(index, blockhash)| {
                let tx = build_transaction_with_config(
                    &self.tx_config,
                    &RpcType::IrisBatch,
                    index,
                    blockhash,
                );
                let sig = *tx.get_signature();
                expected_sigs.push(sig);
                let tx_bytes = bincode::serialize(&tx).expect("cannot serialize tx to bincode");
                info!("------------------------------{}", tx_bytes.len());
                base64::prelude::BASE64_STANDARD.encode(tx_bytes)
            })
            .collect();

        let body = encoded_txns.join(",");

        info!("sending batch of {} txns to {}", indices.len(), self.url);

        let start = Instant::now();
        let response = self
            .client
            .post(&self.url)
            .header("api_key", &self.auth)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;
        let send_elapsed_ms = start.elapsed().as_millis() as u64;

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "sendBatch failed, body {}, status: {}",
                body_text,
                status
            ));
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text)?;
        let result_array = parsed["result"].as_array().ok_or_else(|| {
            anyhow::anyhow!("sendBatch response missing result array: {}", body_text)
        })?;

        let signatures: Vec<TxResult> = result_array
            .iter()
            .map(|v| {
                let sig_str = v.as_str().unwrap_or("");
                let sig = Signature::from_str(sig_str).unwrap_or_else(|_| {
                    panic!("invalid signature in sendBatch response: {}", sig_str)
                });
                TxResult::Signature(sig, send_elapsed_ms)
            })
            .collect();

        info!(
            "sendBatch returned {} signatures from {}",
            signatures.len(),
            self.name
        );
        info!("result: {:?}", result_array);

        Ok(signatures)
    }
}
