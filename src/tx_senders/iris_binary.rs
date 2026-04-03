use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use tokio::time::Instant;

pub struct IrisBinaryTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl IrisBinaryTxSender {
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

    pub fn build_transaction_with_config(&self, index: u32, recent_blockhash: Hash) -> Transaction {
        build_transaction_with_config(&self.tx_config, &RpcType::Iris, index, recent_blockhash)
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    jsonrpc: String,
    id: u64,
    result: String,
}
#[async_trait]
impl TxSender for IrisBinaryTxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let tx = self.build_transaction_with_config(index, recent_blockhash);
        let signature = tx.get_signature();
        let tx_bytes = bincode::serialize(&tx).context("cannot serialize tx to bincode")?;
        // info!("sending to url: {}", self.url);
        let start = Instant::now();
        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/octet-stream")
            .body(tx_bytes)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        let send_elapsed_ms = start.elapsed().as_millis() as u64;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "failed to send tx, body {}, status: {}",
                body,
                status
            ));
        }
        Ok(TxResult::Signature(*signature, send_elapsed_ms))
    }
}
