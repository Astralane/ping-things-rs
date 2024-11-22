use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

pub struct BlockXRouteTxSender {
    url: String,
    name: String,
    auth: String,
    client: Client,
    tx_config: TransactionConfig,
}

impl BlockXRouteTxSender {
    pub fn new(
        name: String,
        url: String,
        auth: String,
        tx_config: TransactionConfig,
        client: Client,
    ) -> Self {
        Self {
            url,
            name,
            auth,
            client,
            tx_config,
        }
    }

    pub fn build_transaction_with_config(&self, index: u32, recent_blockhash: Hash) -> Transaction {
        build_transaction_with_config(
            &self.tx_config,
            &RpcType::BlockXRoute,
            index,
            recent_blockhash,
        )
    }
}

#[derive(Deserialize)]
struct BlockxRouteResponse {
    signature: String,
}

#[async_trait]
impl TxSender for BlockXRouteTxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let tx = self.build_transaction_with_config(index, recent_blockhash);
        let tx_bytes = bincode::serialize(&tx).context("cannot serialize tx to bincode")?;
        let tx_str = base64::encode(tx_bytes);
        let body = json!({
            "transaction": {
                "content": tx_str,
            },
            "useStakedRPCs": true,
        });
        let response = self
            .client
            .post(&self.url)
            .header("Authorization", self.auth.clone())
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("failed to send tx: {}", body));
        }
        let parsed_resp = serde_json::from_str::<BlockxRouteResponse>(&body)
            .context("cannot deserialize signature")?;
        let sig = Signature::from_str(&parsed_resp.signature).context("cannot parse signature")?;
        Ok(TxResult::Signature(sig))
    }
}
