use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use anyhow::Context;
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::bs58;
use solana_sdk::hash::Hash;
use solana_sdk::transaction::Transaction;
use tracing::{debug, info};

pub struct JitoTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl JitoTxSender {
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
        build_transaction_with_config(&self.tx_config, &RpcType::Jito, index, recent_blockhash)
    }
}

#[derive(Deserialize)]
pub struct JitoBundleStatusResponseInnerContext {
    pub slot: u64,
}

#[derive(Deserialize)]
pub struct JitoBundleStatusResponseInnerValue {
    pub slot: u64,
    pub bundle_id: String,
    pub transactions: Vec<String>,
    pub confirmation_status: String,
    pub err: Value,
}

#[derive(Deserialize)]
pub struct JitoBundleStatusResponseInner {
    pub context: JitoBundleStatusResponseInnerContext,
    pub value: Vec<JitoBundleStatusResponseInnerValue>,
}
#[derive(Deserialize)]
pub struct JitoBundleStatusResponse {
    pub result: JitoBundleStatusResponseInner,
}

#[derive(Deserialize)]
pub struct JitoResponse {
    //bundle id is response
    pub result: String,
}

#[async_trait]
impl TxSender for JitoTxSender {
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
        let encoded_transaction = base64::prelude::BASE64_STANDARD
            .encode(tx_bytes);
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [encoded_transaction, {
                "encoding": "base64",
                }]
        });
        debug!("sending tx: {}", body.to_string());
        let tx_url = format!("{}/api/v1/transactions?uuid={}", self.url, self.auth);
        let response = self
            .client
            .post(&tx_url)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "failed to send tx, body {}, status: {}",
                body,
                status
            ));
        }
        Ok(TxResult::Signature(signature.clone()))
    }
}
