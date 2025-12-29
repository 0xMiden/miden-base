use std::hint::black_box;
use std::time::Duration;

use anyhow::Result;
use bench_transaction::context_setups::{tx_consume_single_p2id_note, tx_consume_two_p2id_notes};
use criterion::{BatchSize, Criterion, SamplingMode, criterion_group, criterion_main};
use miden_protocol::transaction::{ExecutedTransaction, ProvenTransaction};
use miden_tx::{HashFunction, LocalTransactionProver, ProvingOptions};

// BENCHMARK NAMES
// ================================================================================================

const BENCH_GROUP_EXECUTE: &str = "Execute transaction";
const BENCH_EXECUTE_TX_CONSUME_SINGLE_P2ID: &str =
    "Execute transaction which consumes single P2ID note";
const BENCH_EXECUTE_TX_CONSUME_TWO_P2ID: &str = "Execute transaction which consumes two P2ID notes";

// CORE PROVING BENCHMARKS
// ================================================================================================

fn execute_benchmarks(c: &mut Criterion) {
    // EXECUTE GROUP (no proving, just execution)
    // --------------------------------------------------------------------------------------------

    let mut execute_group = c.benchmark_group(BENCH_GROUP_EXECUTE);

    execute_group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(1000));

    execute_group.bench_function(BENCH_EXECUTE_TX_CONSUME_SINGLE_P2ID, |b| {
        b.to_async(tokio::runtime::Builder::new_current_thread().build().unwrap())
            .iter_batched(
                || {
                    // prepare the transaction context
                    tx_consume_single_p2id_note()
                        .expect("failed to create a context which consumes single P2ID note")
                },
                |tx_context| async move {
                    // benchmark the transaction execution
                    black_box(tx_context.execute().await)
                },
                BatchSize::SmallInput,
            );
    });

    execute_group.bench_function(BENCH_EXECUTE_TX_CONSUME_TWO_P2ID, |b| {
        b.to_async(tokio::runtime::Builder::new_current_thread().build().unwrap())
            .iter_batched(
                || {
                    // prepare the transaction context
                    tx_consume_two_p2id_notes()
                        .expect("failed to create a context which consumes two P2ID notes")
                },
                |tx_context| async move {
                    // benchmark the transaction execution
                    black_box(tx_context.execute().await)
                },
                BatchSize::SmallInput,
            );
    });

    execute_group.finish();
}

fn prove_benchmarks_blake3(c: &mut Criterion) {
    prove_with_hash_function(c, HashFunction::Blake3_256, "Blake3_256");
}

fn prove_benchmarks_rpo256(c: &mut Criterion) {
    prove_with_hash_function(c, HashFunction::Rpo256, "Rpo256");
}

fn prove_benchmarks_poseidon2(c: &mut Criterion) {
    prove_with_hash_function(c, HashFunction::Poseidon2, "Poseidon2");
}

fn prove_with_hash_function(c: &mut Criterion, hash_fn: HashFunction, hash_name: &str) {
    let group_name = format!("Prove transaction ({})", hash_name);
    let mut prove_group = c.benchmark_group(&group_name);

    prove_group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(1000));

    let bench_name_single = format!("Prove single P2ID note ({})", hash_name);
    let bench_name_two = format!("Prove two P2ID notes ({})", hash_name);

    prove_group.bench_function(&bench_name_single, |b| {
        b.to_async(tokio::runtime::Builder::new_current_thread().build().unwrap())
            .iter_batched(
                || {
                    // prepare the transaction context
                    tx_consume_single_p2id_note()
                        .expect("failed to create a context which consumes single P2ID note")
                },
                |tx_context| async move {
                    // benchmark the transaction execution and proving
                    let executed_tx = tx_context
                        .execute()
                        .await
                        .expect("execution of the single P2ID note consumption tx failed");
                    black_box(prove_transaction(executed_tx, hash_fn))
                },
                BatchSize::SmallInput,
            );
    });

    prove_group.bench_function(&bench_name_two, |b| {
        b.to_async(tokio::runtime::Builder::new_current_thread().build().unwrap())
            .iter_batched(
                || {
                    // prepare the transaction context
                    tx_consume_two_p2id_notes()
                        .expect("failed to create a context which consumes two P2ID notes")
                },
                |tx_context| async move {
                    // benchmark the transaction execution and proving
                    let executed_tx = tx_context
                        .execute()
                        .await
                        .expect("execution of the two P2ID note consumption tx failed");
                    black_box(prove_transaction(executed_tx, hash_fn))
                },
                BatchSize::SmallInput,
            );
    });

    prove_group.finish();
}

fn prove_transaction(
    executed_transaction: ExecutedTransaction,
    hash_fn: HashFunction,
) -> Result<()> {
    let executed_transaction_id = executed_transaction.id();
    let proof_options = ProvingOptions::with_96_bit_security(hash_fn);
    let proven_transaction: ProvenTransaction =
        LocalTransactionProver::new(proof_options).prove(executed_transaction)?;

    assert_eq!(proven_transaction.id(), executed_transaction_id);
    Ok(())
}

criterion_group!(
    benches,
    execute_benchmarks,
    prove_benchmarks_blake3,
    prove_benchmarks_rpo256,
    prove_benchmarks_poseidon2
);
criterion_main!(benches);
