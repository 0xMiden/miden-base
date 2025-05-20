use std::path::PathBuf;

use bench_prover::{
    benchmark_names::{BENCH_CONSUME_MULTIPLE_NOTES, BENCH_CONSUME_NOTE_NEW_ACCOUNT, BENCH_GROUP},
    utils::{cargo_target_directory, process_benchmark_data, save_json_to_file},
};
use serde_json::json;

fn main() -> std::io::Result<()> {
    let target_dir = cargo_target_directory().unwrap_or_else(|| PathBuf::from("target"));
    let base_path = target_dir.join("criterion").join(BENCH_GROUP);

    println!("Looking for benchmark results in: {}", base_path.display());

    let benchmarks = vec![BENCH_CONSUME_NOTE_NEW_ACCOUNT, BENCH_CONSUME_MULTIPLE_NOTES];

    let mut consolidated_results = json!({});

    for benchmark in benchmarks {
        let benchmark_path = base_path.join(benchmark).join("new");

        println!("\nProcessing benchmark: {benchmark}");

        if !benchmark_path.exists() {
            println!("Directory does not exist: {}", benchmark_path.display());
            continue;
        }

        match process_benchmark_data(&benchmark_path) {
            Ok(benchmark_data) => {
                consolidated_results[benchmark] = benchmark_data;
            },
            Err(err) => {
                println!("Error processing benchmark data: {err}");
            },
        }
    }

    let output_path = target_dir.join("criterion").join("consolidated_benchmarks.json");
    if let Err(err) = save_json_to_file(&consolidated_results, &output_path) {
        println!("Error saving JSON file: {err}");
    }

    Ok(())
}
