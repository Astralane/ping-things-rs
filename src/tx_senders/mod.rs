use crate::config::{RpcConfig, RpcType};
use crate::tx_senders::solana_rpc::SolanaRpcTxSender;
use crate::tx_senders::transaction::TransactionConfig;
use async_trait::async_trait;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::sync::Arc;

pub mod blockxroute;
mod constants;
pub mod solana_rpc;
pub mod transaction;

#[async_trait]
pub trait TxSender: Sync + Send {
    fn name(&self) -> String;
    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<Signature>;
}

pub fn create_tx_sender(
    name: String,
    rpc_config: RpcConfig,
    tx_config: TransactionConfig,
) -> Arc<dyn TxSender> {
    match rpc_config.rpc_type {
        RpcType::BlockXRoute => {
            let tx_sender = blockxroute::BlockXRouteTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("blockxroute requieres auth"),
                tx_config,
            );
            Arc::new(tx_sender)
        }
        RpcType::SolanaRpc => {
            let tx_sender = SolanaRpcTxSender::new(name, rpc_config.url, tx_config);
            Arc::new(tx_sender)
        }
        _ => {
            panic!("Unsupported tx sender");
        }
    }
}
