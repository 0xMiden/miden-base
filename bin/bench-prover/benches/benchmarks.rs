use std::{hint::black_box, time::Duration};

use bench_prover::{
    bench_functions::{
        prove_transaction, setup_consume_multiple_notes, setup_consume_note_with_new_account,
    },
    benchmark_names::{BENCH_CONSUME_MULTIPLE_NOTES, BENCH_CONSUME_NOTE_NEW_ACCOUNT, BENCH_GROUP},
};
use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};

fn core_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group(BENCH_GROUP);

    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(1000));

    group.bench_function(BENCH_CONSUME_NOTE_NEW_ACCOUNT, |b| {
        let executed_transaction = setup_consume_note_with_new_account()
            .expect("Failed to set up transaction for consuming note with new account");

        // Only benchmark proving and verification
        b.iter(|| black_box(prove_transaction(executed_transaction.clone())));
    });

    group.bench_function(BENCH_CONSUME_MULTIPLE_NOTES, |b| {
        let executed_transaction = setup_consume_multiple_notes()
            .expect("Failed to set up transaction for consuming multiple notes");

        // Only benchmark the proving and verification
        b.iter(|| black_box(prove_transaction(executed_transaction.clone())));
    });

    group.finish();
}
criterion_group!(benches, core_benchmarks);
criterion_main!(benches);
