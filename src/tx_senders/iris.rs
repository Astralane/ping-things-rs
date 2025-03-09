use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use solana_client::rpc_client::SerializableTransaction;
use solana_rpc_client_api::config::RpcSendTransactionConfig;
use solana_sdk::bs58;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use base64::Engine;
use solana_transaction_status::UiTransactionEncoding;
use tracing::{debug, info};

pub struct IrisTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl IrisTxSender {
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
impl TxSender for IrisTxSender {
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
        let encoded_transaction = base64::prelude::BASE64_STANDARD.encode(tx_bytes);
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: None,
            encoding: Some(UiTransactionEncoding::Base64),
            max_retries: None,
            min_context_slot: None,
        };
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [encoded_transaction, config]
        });
        debug!("sending tx: {}", body.to_string());
        // info!("sending to url: {}", self.url);
        let response = self
            .client
            .post(&self.url)
            .header("api_key", &self.auth)
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
        let response: RpcResponse = serde_json::from_str(&body)?;
        let signature_response = Signature::from_str(&response.result)?;
        info!("signature got back from iris: {}", signature_response);
        assert_eq!(signature, &signature_response);
        Ok(TxResult::Signature(signature_response))
    }
}
