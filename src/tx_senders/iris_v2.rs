use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use anyhow::Context;
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use tracing::{debug, info};

#[derive(Clone, Copy)]
pub enum Encoding {
    Base64Text,
    Binary,
}

pub struct IrisV2TxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
    encoding: Encoding,
}

impl IrisV2TxSender {
    pub fn new(
        name: String,
        url: String,
        auth: String,
        tx_config: TransactionConfig,
        client: Client,
        encoding: Encoding,
    ) -> Self {
        Self {
            url,
            name,
            auth,
            client,
            tx_config,
            encoding,
        }
    }
}

#[derive(Deserialize)]
struct V2Response {
    result: String,
}

#[async_trait]
impl TxSender for IrisV2TxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let tx = build_transaction_with_config(
            &self.tx_config,
            &RpcType::Iris2,
            index,
            recent_blockhash,
        );
        let signature = *tx.get_signature();
        let tx_bytes = bincode::serialize(&tx).context("cannot serialize tx to bincode")?;

        let request = self
            .client
            .post(&self.url)
            .query(&[("api-key", self.auth.as_str()), ("method", "sendTransaction")]);
        let request = match self.encoding {
            Encoding::Base64Text => request
                .header("Content-Type", "text/plain")
                .body(base64::prelude::BASE64_STANDARD.encode(tx_bytes)),
            Encoding::Binary => request
                .header("Content-Type", "application/octet-stream")
                .body(tx_bytes),
        };
        debug!("sending tx {} to {}", signature, self.url);

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "failed to send tx, body {}, status: {}",
                body,
                status
            ));
        }
        let response: V2Response =
            serde_json::from_str(&body).with_context(|| format!("unexpected body {}", body))?;
        let signature_response = Signature::from_str(&response.result)?;
        info!("signature got back from {}: {}", self.name, signature_response);
        anyhow::ensure!(
            signature == signature_response,
            "signature mismatch: sent {} got {}",
            signature,
            signature_response
        );
        Ok(TxResult::Signature(signature_response))
    }
}
