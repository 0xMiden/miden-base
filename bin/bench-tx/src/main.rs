use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use miden_objects::transaction::TransactionMeasurements;

mod tx_variants;
use tx_variants::{tx_consume_multiple_p2id_notes, tx_consume_p2id, tx_create_p2id};

mod execution_benchmarks;
use execution_benchmarks::ExecutionBenchmark;
use execution_benchmarks::utils::write_bench_results_to_json;

fn main() -> Result<()> {
    // create a template file for benchmark results
    let path = Path::new("bin/bench-tx/bench-tx.json");
    let mut file = File::create(path).context("failed to create file")?;
    file.write_all(b"{}").context("failed to write to file")?;

    // run all available benchmarks
    let benchmark_results = vec![
        (
            ExecutionBenchmark::ConsumeSingleP2ID,
            tx_consume_p2id().map(TransactionMeasurements::from)?.into(),
        ),
        (
            ExecutionBenchmark::ConsumeMultipleP2ID,
            tx_consume_multiple_p2id_notes().map(TransactionMeasurements::from)?.into(),
        ),
        (
            ExecutionBenchmark::CreateSingleP2ID,
            tx_create_p2id().map(TransactionMeasurements::from)?.into(),
        ),
    ];

    // store benchmark results in the JSON file
    write_bench_results_to_json(path, benchmark_results)?;

    Ok(())
}
