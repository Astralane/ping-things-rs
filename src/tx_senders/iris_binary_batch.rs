use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use async_trait::async_trait;
use reqwest::Client;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use tracing::info;

pub struct IrisBinaryBatchTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl IrisBinaryBatchTxSender {
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
impl TxSender for IrisBinaryBatchTxSender {
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

    /// Send multiple transactions in a binary batch format.
    async fn send_batch(
        &self,
        indices: &[(u32, Hash)],
    ) -> anyhow::Result<Vec<TxResult>> {
        let mut expected_sigs = Vec::with_capacity(indices.len());
        let mut tx_data_vec = Vec::with_capacity(indices.len());

        for &(index, blockhash) in indices {
            let tx = build_transaction_with_config(&self.tx_config, &RpcType::IrisBatch, index, blockhash);
            let sig = *tx.get_signature();
            info!("sig : {}", sig.to_string());
            expected_sigs.push(sig);
            let tx_bytes = bincode::serialize(&tx).expect("cannot serialize tx to bincode");
            tx_data_vec.push(tx_bytes);
        }

        let mut binary_body = Vec::new();

        for tx_bytes in &tx_data_vec {
            let len = tx_bytes.len() as u16;
            binary_body.extend_from_slice(&len.to_be_bytes());
            binary_body.extend_from_slice(tx_bytes);
        }

        info!("sending binary batch of {} txns to {}", indices.len(), self.url);

        let response = self
            .client
            .post(&self.url)
            .header("x-api-key", &self.auth)
            .header("content-type", "application/octet-stream")
            .body(binary_body)
            .send()
            .await?;

        let status = response.status();
        let body_text = response.text().await?;
        println!("{}", body_text);
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "binary batch failed, body {}, status: {}",
                body_text,
                status
            ));
        }


        let signatures: Vec<TxResult> = if body_text.trim().starts_with('{') || body_text.trim().starts_with('[') {
            let parsed: serde_json::Value = serde_json::from_str(&body_text)?;
            if let Some(result_array) = parsed["result"].as_array() {
                result_array
                    .iter()
                    .map(|v| {
                        let sig_str = v.as_str().unwrap_or("");
                        let sig = Signature::from_str(sig_str)
                            .unwrap_or_else(|_| panic!("invalid signature in batch response: {}", sig_str));
                        TxResult::Signature(sig)
                    })
                    .collect()
            } else if let Some(result_array) = parsed.as_array() {
                result_array
                    .iter()
                    .map(|v| {
                        let sig_str = v.as_str().unwrap_or("");
                        let sig = Signature::from_str(sig_str)
                            .unwrap_or_else(|_| panic!("invalid signature in batch response: {}", sig_str));
                        TxResult::Signature(sig)
                    })
                    .collect()
            } else {
                return Err(anyhow::anyhow!("unexpected JSON response format: {}", body_text));
            }
        } else {

            expected_sigs.into_iter().map(TxResult::Signature).collect()
        };

        info!(
            "binary batch returned {} signatures from {}",
            signatures.len(),
            self.name
        );

        Ok(signatures)
    }
}
