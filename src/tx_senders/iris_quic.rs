use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use async_trait::async_trait;
use astralane_quic_client::AstralaneQuicClient;
use solana_client::rpc_client::SerializableTransaction;
use solana_sdk::hash::Hash;
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

pub struct IrisQuicTxSender {
    endpoint: String,  // "lim.gateway.astralane.io:7000" 
    api_key: String,
    name: String,
    tx_config: TransactionConfig,
}

impl IrisQuicTxSender {
    pub fn new(
        name: String,
        endpoint: String,
        api_key: String,
        tx_config: TransactionConfig,
    ) -> Self {
        Self {
            endpoint,
            api_key,
            name,
            tx_config,
        }
    }

    /// Connect → Send → Delay → Close pattern as recommended in the astralane-quic-client docs
    async fn send_with_quic(&self, tx_bytes: &[u8]) -> anyhow::Result<()> {
        info!(
            "connecting to QUIC endpoint {} for {}",
            self.endpoint, self.name
        );

        let client = AstralaneQuicClient::connect(&self.endpoint, &self.api_key).await
            .map_err(|e| anyhow::anyhow!("failed to connect to QUIC endpoint: {}", e))?;

        client.send_transaction(tx_bytes).await
            .map_err(|e| anyhow::anyhow!("failed to send transaction via QUIC: {}", e))?;

        // Small delay before close to let the server read in-flight streams
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        client.close().await;
        
        info!("successfully sent transaction via QUIC and closed connection");
        Ok(())
    }
}

#[async_trait]
impl TxSender for IrisQuicTxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_batch(&self) -> bool {
        false // QUIC is single-transaction focused
    }

    /// Send single transaction via QUIC connection
    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let tx = build_transaction_with_config(
            &self.tx_config,
            &RpcType::IrisQuic,
            index,
            recent_blockhash,
        );

        let sig = *tx.get_signature();
        info!("sending tx via QUIC with sig: {}", sig.to_string());

        let tx_bytes = bincode::serialize(&tx)
            .map_err(|e| anyhow::anyhow!("failed to serialize transaction: {}", e))?;

        let start = Instant::now();
        self.send_with_quic(&tx_bytes).await?;
        let send_elapsed_ms = start.elapsed().as_millis() as u64;

        info!(
            "QUIC transaction sent, expected sig: {} ({}ms)",
            sig, send_elapsed_ms
        );

        // Fire-and-forget: return expected signature
        Ok(TxResult::Signature(sig, send_elapsed_ms))
    }

    /// Send batch as sequential QUIC transactions
    async fn send_batch(&self, indices: &[(u32, Hash)]) -> anyhow::Result<Vec<TxResult>> {
        info!(
            "sending batch of {} transactions via QUIC to {}",
            indices.len(),
            self.name
        );

        let mut results = Vec::with_capacity(indices.len());

        for &(index, blockhash) in indices {
            let result = self.send_transaction(index, blockhash).await?;
            results.push(result);
        }

        info!(
            "QUIC batch completed: {} transactions sent to {}",
            results.len(),
            self.name
        );

        Ok(results)
    }
}