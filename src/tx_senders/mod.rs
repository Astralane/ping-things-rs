use crate::config::{RpcConfig, RpcType};
use crate::tx_senders::jito::JitoTxSender;
use crate::tx_senders::solana_rpc::GenericRpc;
use crate::tx_senders::transaction::TransactionConfig;
use async_trait::async_trait;
use reqwest::Client;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::sync::Arc;

pub mod blockxroute;
pub mod constants;
pub mod jito;
pub mod solana_rpc;
pub mod transaction;

#[derive(Debug, Clone)]
pub enum TxResult {
    Signature(Signature),
    BundleID(String),
}

impl Into<String> for TxResult {
    fn into(self) -> String {
        match self {
            TxResult::Signature(sig) => sig.to_string(),
            TxResult::BundleID(bundle_id) => bundle_id,
        }
    }
}

#[async_trait]
pub trait TxSender: Sync + Send {
    fn name(&self) -> String;
    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult>;
}

pub fn create_tx_sender(
    name: String,
    rpc_config: RpcConfig,
    tx_config: TransactionConfig,
    client: Client,
) -> Arc<dyn TxSender> {
    match rpc_config.rpc_type {
        RpcType::BlockXRoute => {
            let tx_sender = blockxroute::BlockXRouteTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("blockxroute requieres auth"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::SolanaRpc => {
            let tx_sender = GenericRpc::new(name, rpc_config.url, tx_config, RpcType::SolanaRpc);
            Arc::new(tx_sender)
        }
        RpcType::Temporal => {
            let tx_sender = GenericRpc::new(name, rpc_config.url, tx_config, RpcType::Temporal);
            Arc::new(tx_sender)
        }
        RpcType::Jito => {
            let tx_sender = JitoTxSender::new(name, rpc_config.url, tx_config, client);
            Arc::new(tx_sender)
        }
    }
}
