use anyhow::Context;
use rand::Rng;
use serde::Serialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcSendTransactionConfig;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::message::Message;
use solana_sdk::signature::{Keypair, Signature, Signer};
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use solana_transaction_status::UiTransactionEncoding;
use std::sync::Arc;

#[derive(Clone)]
pub struct RpcTxSender {
    pub name: String,
    pub http_rpc: Arc<RpcClient>,
    tx_config: TxConfig,
}

#[derive(Clone)]
pub struct TxConfig {
    keypair: Arc<Keypair>,
    compute_unit_limit: u32,
    compute_unit_price: u64,
    enable_priority_fee: bool,
}

#[derive(Serialize, Debug)]
pub struct TxMetrics {
    pub rpc_name: String,
    pub signature: String,
    pub index: u32,
    pub success: bool,
    pub slot_sent: u64,
    pub slot_landed: Option<u64>,
    pub slot_latency: Option<u64>,
    pub elapsed: Option<u64>, // in milliseconds
}
impl RpcTxSender {
    pub fn new(
        name: String,
        url: String,
        keypair: Keypair,
        compute_unit_limit: u32,
        compute_unit_price: u64,
        enable_priority_fee: bool,
    ) -> Self {
        let http_rpc = Arc::new(RpcClient::new(url));
        RpcTxSender {
            name,
            http_rpc,
            tx_config: TxConfig {
                keypair: Arc::new(keypair),
                compute_unit_limit,
                compute_unit_price,
                enable_priority_fee,
            },
        }
    }

    fn build_transaction_with_config(&self, index: u32, recent_blockhash: Hash) -> Transaction {
        let pay = (5000 + index) as u64 + (rand::thread_rng().gen_range(1..1000 /* high */));

        let transfer_instruction = system_instruction::transfer(
            &self.tx_config.keypair.pubkey(),
            &self.tx_config.keypair.pubkey(),
            pay,
        );
        let compute_unit_limit =
            ComputeBudgetInstruction::set_compute_unit_limit(self.tx_config.compute_unit_limit);
        let compute_unit_price =
            ComputeBudgetInstruction::set_compute_unit_price(self.tx_config.compute_unit_price);
        let message;
        if self.tx_config.enable_priority_fee {
            message = Message::new(
                &[compute_unit_limit, compute_unit_price, transfer_instruction],
                Some(&self.tx_config.keypair.pubkey()),
            );
        } else {
            message = Message::new(
                &[transfer_instruction],
                Some(&self.tx_config.keypair.pubkey()),
            );
        }
        // Create transaction
        Transaction::new(&[&self.tx_config.keypair], message, recent_blockhash)
    }

    pub async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<Signature> {
        let transaction = self.build_transaction_with_config(index, recent_blockhash);
        self.http_rpc
            .send_transaction_with_config(
                &transaction,
                RpcSendTransactionConfig {
                    skip_preflight: false,
                    preflight_commitment: None,
                    encoding: Some(UiTransactionEncoding::Base64),
                    max_retries: None,
                    min_context_slot: None,
                },
            )
            .await
            .context(format!("Failed to send transaction for {}", self.name))
    }
}
