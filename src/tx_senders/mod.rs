use crate::config::{RpcConfig, RpcType};
use crate::tx_senders::jito::JitoTxSender;
use crate::tx_senders::solana_rpc::GenericRpc;
use crate::tx_senders::transaction::TransactionConfig;
use async_trait::async_trait;
use reqwest::Client;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Signature;
use std::sync::Arc;

mod blockrazer;
pub mod blockxroute;
pub mod constants;
mod fast;
mod flashblock;
mod iris;
mod iris_paladin;
pub mod jito;
mod nextblock;
mod node1;
pub mod solana_rpc;
pub mod transaction;
mod zero_slot;

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
        RpcType::Helius => {
            let tx_sender = GenericRpc::new(name, rpc_config.url, tx_config, RpcType::Helius);
            Arc::new(tx_sender)
        }
        RpcType::Jito => {
            let tx_sender = JitoTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for jito"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::Iris => {
            let tx_sender = iris::IrisTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisPaladin => {
            let tx_sender = iris_paladin::IrisTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris paladin"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::ZeroSlot => {
            let tx_sender =
                zero_slot::ZeroSlotTxSender::new(name, rpc_config.url, tx_config, client);
            Arc::new(tx_sender)
        }
        RpcType::BlockRazer => {
            let tx_sender = blockrazer::BlockRazerSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for blockrazer"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::Flashblock => {
            let tx_sender = flashblock::FlashblockSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for flashblock"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::Node1 => {
            let tx_sender = node1::Node1Sender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for node1"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::NextBlock => {
            let tx_sender = nextblock::NextBlockSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for NextBlock"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::Fast => {
            let tx_sender = fast::FastSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for fast"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
    }
}
