//! Detailed profiling of STARK proof generation phases using Plonky3's built-in tracing.
//!
//! Usage:
//!   RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features concurrent --release
//!
//! With different hash functions:
//!   HASH_FN=blake3 RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features concurrent
//! --release   HASH_FN=poseidon2 RAYON_NUM_THREADS=16 cargo run --bin profile_proving --features
//! concurrent --release
//!
//! For more detailed output (includes merkle tree building):
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

#[tokio::main]
async fn main() -> Result<()> {
    // Set up tracing-forest subscriber with Plonky3 tracing enabled
    // Use RUST_LOG env var or default to info level (top-level spans only)
    // For more detail: RUST_LOG=debug or RUST_LOG=p3_uni_stark=debug,p3_fri=info
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default: info level shows top 2-3 levels of the proving tree
        // miden_tx=info captures execute_for_trace and build_trace spans
        EnvFilter::new("miden_tx=info,miden_prover=info,p3_uni_stark=info,p3_fri=info")
    });

    tracing_subscriber::registry().with(ForestLayer::default()).with(filter).init();

    // Get hash function from env or default to Rpo256
    let hash_fn = match std::env::var("HASH_FN").as_deref() {
        Ok("blake3") | Ok("Blake3_256") => HashFunction::Blake3_256,
        Ok("poseidon2") | Ok("Poseidon2") => HashFunction::Poseidon2,
        _ => HashFunction::Rpo256,
    };

    println!("=== STARK Proof Generation Profiling (Plonky3) ===");
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

    // Prove with tracing - Plonky3's spans will be captured automatically
    println!("Starting proof generation...");
    println!();

    let start = Instant::now();
    let proof_options = ProvingOptions::new(hash_fn);
    let tx_inputs: TransactionInputs = executed_tx.into();
    let _proven_tx: ProvenTransaction =
        LocalTransactionProver::new(proof_options).prove_async(tx_inputs).await?;
    let prove_time = start.elapsed();

    println!();
    println!("Total proving time: {:?}", prove_time);
    println!();
    println!("=== Profiling Complete ===");

    Ok(())
}
