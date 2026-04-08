use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use tokio::time::Instant;
use tracing::info;

pub struct MoonBinaryBatchTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

#[derive(Deserialize)]
struct MoonBatchResponse {
    attempted: u32,
    accepted: u32,
    rejected: u32,
    parse_error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct MoonParseError {
    kind: String,
    at_tx_index: u32,
    message: String,
}

impl MoonBinaryBatchTxSender {
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
impl TxSender for MoonBinaryBatchTxSender {
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

    /// Send multiple transactions in Moon's binary batch format.
    async fn send_batch(&self, indices: &[(u32, Hash)]) -> anyhow::Result<Vec<TxResult>> {
        // Validate Moon's batch size limit
        if indices.len() > 16 {
            return Err(anyhow::anyhow!(
                "Moon batch limit exceeded: {} transactions (max 16)", 
                indices.len()
            ));
        }

        let mut expected_sigs = Vec::with_capacity(indices.len());
        let mut tx_data_vec = Vec::with_capacity(indices.len());

        for &(index, blockhash) in indices {
            let tx = build_transaction_with_config(
                &self.tx_config,
                &RpcType::MoonBinaryBatch,
                index,
                blockhash,
            );
            let sig = *tx.get_signature();
            info!("sig : {}", sig.to_string());
            expected_sigs.push(sig);
            let tx_bytes = bincode::serialize(&tx).expect("cannot serialize tx to bincode");
            
            // Validate Moon's transaction size limits
            if tx_bytes.len() < 66 {
                return Err(anyhow::anyhow!(
                    "Transaction {} too small: {} bytes (min 66)",
                    index, tx_bytes.len()
                ));
            }
            if tx_bytes.len() > 1232 {
                return Err(anyhow::anyhow!(
                    "Transaction {} too large: {} bytes (max 1232)",
                    index, tx_bytes.len()
                ));
            }
            
            tx_data_vec.push(tx_bytes);
        }

        let mut binary_body = Vec::new();

        // Build Moon's wire format: [len1_be][tx1][len2_be][tx2][len3_be][tx3]
        for tx_bytes in &tx_data_vec {
            let len = tx_bytes.len() as u16;
            binary_body.extend_from_slice(&len.to_be_bytes());
            binary_body.extend_from_slice(tx_bytes);
        }

        info!(
            "sending Moon binary batch of {} txns to {}",
            indices.len(),
            self.url
        );

        let start = Instant::now();
        let response = self
            .client
            .post(&self.url)
            .header("authorization", format!("Bearer {}", &self.auth))
            .header("content-type", "application/octet-stream")
            .body(binary_body)
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;
        let send_elapsed_ms = start.elapsed().as_millis() as u64;
        
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Moon binary batch failed, body {}, status: {}",
                body_text,
                status
            ));
        }

        // Parse Moon's response format
        let moon_response: MoonBatchResponse = serde_json::from_str(&body_text)?;

        info!(
            "Moon batch result: attempted={}, accepted={}, rejected={}",
            moon_response.attempted, moon_response.accepted, moon_response.rejected
        );

        // Log parse errors if present
        if let Some(parse_error) = &moon_response.parse_error {
            if let Ok(error_detail) = serde_json::from_value::<MoonParseError>(parse_error.clone()) {
                info!(
                    "Moon parse error at tx {}: {} - {}",
                    error_detail.at_tx_index, error_detail.kind, error_detail.message
                );
            }
        }

        // Moon doesn't return actual signatures, so we use the expected ones
        // and assume that attempted == successful for our purposes
        let signatures: Vec<TxResult> = expected_sigs
            .into_iter()
            .take(moon_response.attempted as usize)
            .map(|sig| TxResult::Signature(sig, send_elapsed_ms))
            .collect();

        info!(
            "Moon binary batch returned status for {} transactions from {}",
            moon_response.attempted,
            self.name
        );

        Ok(signatures)
    }
}