use std::hint::black_box;
use std::time::Duration;

use anyhow::Result;
use bench_transaction::executed_transactions::{tx_consume_single_p2id, tx_consume_two_p2id_notes};
use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};
use miden_objects::transaction::{ExecutedTransaction, ProvenTransaction};
use miden_tx::LocalTransactionProver;

// PROVING BENCHMARK NAMES
// ================================================================================================

const BENCH_CONSUME_NOTE_NEW_ACCOUNT: &str = "prove_consume_note_with_new_account";
const BENCH_CONSUME_MULTIPLE_NOTES: &str = "prove_consume_multiple_notes";
const BENCH_GROUP: &str = "miden_proving";

// CORE PROVING BENCHMARKS
// ================================================================================================

fn core_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group(BENCH_GROUP);

    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(1000));

    group.bench_function(BENCH_CONSUME_NOTE_NEW_ACCOUNT, |b| {
        let executed_transaction = tx_consume_single_p2id()
            .expect("Failed to set up transaction for consuming note with new account");

        // Only benchmark proving and verification
        b.iter(|| black_box(prove_transaction(executed_transaction.clone())));
    });

    group.bench_function(BENCH_CONSUME_MULTIPLE_NOTES, |b| {
        let executed_transaction = tx_consume_two_p2id_notes()
            .expect("Failed to set up transaction for consuming multiple notes");

        // Only benchmark the proving and verification
        b.iter(|| black_box(prove_transaction(executed_transaction.clone())));
    });

    group.finish();
}

fn prove_transaction(executed_transaction: ExecutedTransaction) -> Result<()> {
    let executed_transaction_id = executed_transaction.id();
    let proven_transaction: ProvenTransaction =
        LocalTransactionProver::default().prove(executed_transaction.into())?;

    assert_eq!(proven_transaction.id(), executed_transaction_id);
    Ok(())
}

criterion_group!(benches, core_benchmarks);
criterion_main!(benches);
