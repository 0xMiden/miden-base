use std::path::PathBuf;

use anyhow::{Context, Result};
use bench_prover::{
    benchmark_names::{BENCH_CONSUME_MULTIPLE_NOTES, BENCH_CONSUME_NOTE_NEW_ACCOUNT, BENCH_GROUP},
    utils::{cargo_target_directory, process_benchmark_data, save_json_to_file},
};
use serde_json::json;

fn main() -> Result<()> {
    let target_dir = cargo_target_directory().unwrap_or_else(|| PathBuf::from("target"));
    let base_path = target_dir.join("criterion").join(BENCH_GROUP);

    println!("Looking for benchmark results in: {}", base_path.display());

    let benchmarks = [BENCH_CONSUME_NOTE_NEW_ACCOUNT, BENCH_CONSUME_MULTIPLE_NOTES];

    let mut consolidated_results = json!({});

    for benchmark in benchmarks {
        let benchmark_path = base_path.join(benchmark).join("new");

        println!("\nProcessing benchmark: {benchmark}");

        if !benchmark_path.exists() {
            return Err(anyhow::anyhow!(
                "Benchmark directory does not exist: {}",
                benchmark_path.display()
            ));
        }

        match process_benchmark_data(&benchmark_path) {
            Ok(benchmark_data) => {
                consolidated_results[benchmark] = benchmark_data;
            },
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to process benchmark: {benchmark}"));
            },
        }
    }

    // Rest of the code remains the same
    let output_path = target_dir.join("criterion").join("consolidated_benchmarks.json");
    println!("Writing consolidated file to {}", output_path.display());

    save_json_to_file(&consolidated_results, &output_path)
        .with_context(|| format!("Failed to save results to {}", output_path.display()))?;

    Ok(())
}
