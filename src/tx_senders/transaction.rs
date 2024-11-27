use crate::config::{PingThingsArgs, RpcType};
use crate::tx_senders::constants::{
    BX_MEMO_MARKER_MSG, JITO_TIP_WALLET, NOZOMI_TIP, TRADER_API_MEMO_PROGRAM, TRADER_API_TIP_WALLET,
};
use rand::Rng;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{EncodableKey, Keypair, Signer};
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Clone)]
pub struct TransactionConfig {
    pub keypair: Arc<Keypair>,
    pub compute_unit_limit: u32,
    pub compute_unit_price: u64,
    pub tip: u64,
}

impl From<PingThingsArgs> for TransactionConfig {
    fn from(args: PingThingsArgs) -> Self {
        let keypair =
            Keypair::read_from_file(args.keypair_dir.clone()).expect("cannot read keypair");
        TransactionConfig {
            keypair: Arc::new(keypair),
            compute_unit_limit: args.compute_unit_limit,
            compute_unit_price: args.compute_unit_price,
            tip: args.tip,
        }
    }
}
fn create_trader_api_memo_instruction() -> Instruction {
    Instruction {
        accounts: vec![],
        program_id: Pubkey::from_str(TRADER_API_MEMO_PROGRAM).unwrap(),
        data: BX_MEMO_MARKER_MSG.as_bytes().to_vec(),
    }
}
pub fn build_transaction_with_config(
    tx_config: &TransactionConfig,
    rpc_type: &RpcType,
    index: u32,
    recent_blockhash: Hash,
) -> Transaction {
    let pay = (5000 + index) as u64 + (rand::thread_rng().gen_range(1..1000 /* high */));

    let mut instructions = Vec::new();

    if tx_config.compute_unit_limit > 0 {
        let compute_unit_limit =
            ComputeBudgetInstruction::set_compute_unit_limit(tx_config.compute_unit_limit);
        instructions.push(compute_unit_limit);
    }

    if tx_config.compute_unit_price > 0 {
        let compute_unit_price =
            ComputeBudgetInstruction::set_compute_unit_price(tx_config.compute_unit_price);
        instructions.push(compute_unit_price);
    }

    let memo_instruction = create_trader_api_memo_instruction();
    instructions.push(memo_instruction);

    if tx_config.tip > 0 {
        let tip_instruction = match rpc_type {
            RpcType::BlockXRoute => system_instruction::transfer(
                &tx_config.keypair.pubkey(),
                &Pubkey::from_str(TRADER_API_TIP_WALLET).unwrap(),
                tx_config.tip,
            ),
            RpcType::Jito => system_instruction::transfer(
                &tx_config.keypair.pubkey(),
                &Pubkey::from_str(JITO_TIP_WALLET).unwrap(),
                tx_config.tip,
            ),
            RpcType::SolanaRpc => {
                // add an extra transfer to self
                system_instruction::transfer(
                    &tx_config.keypair.pubkey(),
                    &tx_config.keypair.pubkey(),
                    tx_config.tip,
                )
            }
            RpcType::Temporal => {
                // add an extra transfer to self
                system_instruction::transfer(
                    &tx_config.keypair.pubkey(),
                    &Pubkey::from_str(NOZOMI_TIP).unwrap(),
                    tx_config.tip,
                )
            }
        };
        instructions.push(tip_instruction);
    }

    let self_transfer_instruction = system_instruction::transfer(
        &tx_config.keypair.pubkey(),
        &tx_config.keypair.pubkey(),
        pay,
    );
    instructions.push(self_transfer_instruction);

    let message = Message::new(&instructions, Some(&tx_config.keypair.pubkey()));
    // Create transaction
    Transaction::new(&[&tx_config.keypair], message, recent_blockhash)
}
