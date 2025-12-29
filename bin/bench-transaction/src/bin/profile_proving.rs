//! Detailed profiling of STARK proof generation phases using Winterfell's built-in tracing.
//!
//! Usage:
//!   RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features concurrent --release
//!
//! With different hash functions:
//!   HASH_FN=blake3 RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features concurrent
//! --release   HASH_FN=poseidon2 RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features
//! concurrent --release
//!
//! For more detailed output:
//!   RUST_LOG=debug RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features concurrent
//! --release

use std::time::Instant;

use anyhow::Result;
use bench_transaction::context_setups::tx_consume_single_p2id_note;
use miden_protocol::transaction::{ProvenTransaction, TransactionInputs};
use miden_tx::{HashFunction, LocalTransactionProver, ProvingOptions};
use tracing_forest::ForestLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Set up tracing-forest subscriber with Winterfell tracing enabled
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default: info level for winterfell crates
        EnvFilter::new("winterfell=info,winter_prover=info,miden_prover=info")
    });

    tracing_subscriber::registry().with(ForestLayer::default()).with(filter).init();

    // Get hash function from env or default to Rpo256
    let hash_fn = match std::env::var("HASH_FN").as_deref() {
        Ok("blake3") | Ok("Blake3_256") => HashFunction::Blake3_256,
        Ok("poseidon2") | Ok("Poseidon2") => HashFunction::Poseidon2,
        _ => HashFunction::Rpo256,
    };

    println!("=== STARK Proof Generation Profiling (Winterfell) ===");
    println!("Hash function: {:?}", hash_fn);
    println!();

    // Prepare transaction
    println!("Preparing transaction context...");
    let tx_context = tx_consume_single_p2id_note()?;

    println!("Executing transaction...");
    let start = Instant::now();
    let executed_tx = tx_context.execute().await?;
    let exec_time = start.elapsed();
    println!("Execution time: {:?}", exec_time);
    println!();

    // Convert to TransactionInputs (aligns with Plonky3 API)
    let tx_inputs: TransactionInputs = executed_tx.into();

    // Prove with tracing - Winterfell's spans will be captured automatically
    println!("Starting proof generation...");
    println!();

    let start = Instant::now();
    let proof_options = ProvingOptions::with_96_bit_security(hash_fn);
    let _proven_tx: ProvenTransaction =
        LocalTransactionProver::new(proof_options).prove(tx_inputs)?;
    let prove_time = start.elapsed();

    println!();
    println!("Total proving time: {:?}", prove_time);
    println!();
    println!("=== Profiling Complete ===");

    Ok(())
}
