use std::path::PathBuf;
use tape_api::prelude::InstructionType;
use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
use solana_compute_budget::compute_budget::ComputeBudget;
use litesvm::{types::{TransactionMetadata, TransactionResult}, LiteSVM};
use pretty_hex::*;
use bincode;

pub fn program_bytes() -> Vec<u8> {
   // Fetch the tape program bytes from target
   let mut so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
   so_path.push("../target/deploy/tape.so");
   std::fs::read(so_path).unwrap()
}

pub fn metadata_bytes() -> Vec<u8> {
    // Fetch the metadata program bytes from elfs/ dir before running the test
    // solana program dump --url mainnet-beta metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s metadata.so
    let mut so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    so_path.push("tests/elfs/metadata.so");
    std::fs::read(so_path).unwrap()
}

pub fn setup_svm() -> LiteSVM {
    let mut svm = LiteSVM::new().with_compute_budget(ComputeBudget {
        compute_unit_limit: 1_000_000,
        ..Default::default()
    });

    svm.add_program(mpl_token_metadata::ID, &metadata_bytes());
    svm.add_program(tape_api::ID, &program_bytes());

    svm
}

pub fn send_tx(svm: &mut LiteSVM, tx: Transaction) -> TransactionResult {
    let res = svm.send_transaction(tx.clone());

    let meta = match res.as_ref() {
        Ok(v) => v.clone(),
        Err(v) => v.meta.clone()
    };

    print_tx(meta, tx);

    if res.is_err() {
        println!("error:\t{:?}", res.as_ref().err().unwrap().err);
    }

    res.clone()
}

pub fn create_payer(svm: &mut LiteSVM) -> Keypair {
    let payer_kp = Keypair::new();
    let payer_pk = payer_kp.pubkey();
    svm.airdrop(&payer_pk, 1_000_000_000).unwrap();
    payer_kp
}

pub fn create_keypair() -> Keypair {
    Keypair::new()
}

pub fn get_tx_size(tx: &Transaction) -> usize {
    bincode::serialize(tx).unwrap().len()
}

pub fn print_tx(meta: TransactionMetadata, tx: Transaction) {
    let msg = tx.message().serialize();

    println!("\n");
    println!("--------------------------------------------------------------------------------");
    println!("sig:\t{:?}", meta.signature);
    println!("len:\t{:?}", msg.len());

    for i in 0..tx.message.instructions.len() {
        let ix = &tx.message.instructions[i];
        let ix_type = InstructionType::try_from(ix.data[0]).unwrap();

        println!("\nix:\t{:?} ({})", ix_type, ix.data[0]);
        println!("accounts:");

        for key in &ix.accounts {
            println!("\t{}: {:?}", key, tx.message.account_keys[*key as usize]);
        }

        println!("\ndata:\n\t{:?}", ix.data);
        println!("\n\n{}\n", pretty_hex(&ix.data))
    }

    println!();
    println!("size:\t{:?}", get_tx_size(&tx));
    println!("cu:\t{:?}", meta.compute_units_consumed);
    println!("logs:");
    for log in &meta.logs {
        println!("\t{log:?}");
    }
    println!();
}
