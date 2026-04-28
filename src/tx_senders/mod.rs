use crate::config::{RpcConfig, RpcType};
use crate::shred_listener::ShredMap;
use crate::tx_senders::iris_quic::IrisQuicTxSender;
use crate::tx_senders::iris_rs_quic::IrisRsQuicTxSender;
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
mod iris;
mod iris_batch;
mod iris_binary;
mod iris_binary_batch;
mod iris_paladin;
mod iris_plain_text_batch;
pub mod iris_quic;
pub mod iris_rs_quic;
mod moon_binary_batch;
pub mod jito;
pub mod solana_rpc;
pub mod transaction;

#[derive(Debug, Clone)]
pub enum TxResult {
    Signature(Signature, u64),   // (sig, send_elapsed_ms)
    BundleID(String, u64),       // (bundle_id, send_elapsed_ms)
}

impl TxResult {
    pub fn send_elapsed_ms(&self) -> u64 {
        match self {
            TxResult::Signature(_, ms) => *ms,
            TxResult::BundleID(_, ms) => *ms,
        }
    }
}

impl Into<String> for TxResult {
    fn into(self) -> String {
        match self {
            TxResult::Signature(sig, _) => sig.to_string(),
            TxResult::BundleID(bundle_id, _) => bundle_id,
        }
    }
}

#[async_trait]
pub trait TxSender: Sync + Send {
    fn name(&self) -> String;

    /// Whether this sender uses batch mode (sendBatch API).
    fn is_batch(&self) -> bool {
        false
    }

    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult>;

    /// Send multiple transactions in a single API call.
    /// Default implementation sends them individually.
    async fn send_batch(&self, indices: &[(u32, Hash)]) -> anyhow::Result<Vec<TxResult>> {
        let mut results = Vec::with_capacity(indices.len());
        for &(index, blockhash) in indices {
            results.push(self.send_transaction(index, blockhash).await?);
        }
        Ok(results)
    }
}

pub fn create_tx_sender(
    name: String,
    rpc_config: RpcConfig,
    tx_config: TransactionConfig,
    client: Client,
    shred_map: ShredMap,
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
                shred_map,
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
        RpcType::IrisBatch => {
            let tx_sender = iris_batch::IrisBatchTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris batch"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisPlainTextBatch => {
            let tx_sender = iris_plain_text_batch::IrisBatchTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris batch"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisBinaryBatch => {
            let tx_sender = iris_binary_batch::IrisBinaryBatchTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris batch"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisBinary => {
            let tx_sender = iris_binary::IrisBinaryTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::MoonBinaryBatch => {
            let tx_sender = moon_binary_batch::MoonBinaryBatchTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for moon"),
                tx_config,
                client,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisQuic => {
            let tx_sender = IrisQuicTxSender::new(
                name,
                rpc_config.url,
                rpc_config.auth.expect("use api key for iris quic"),
                tx_config,
            );
            Arc::new(tx_sender)
        }
        RpcType::IrisRsQuic => {
            let tx_sender = IrisRsQuicTxSender::new(name, rpc_config.url, tx_config);
            Arc::new(tx_sender)
        }
    }
}
