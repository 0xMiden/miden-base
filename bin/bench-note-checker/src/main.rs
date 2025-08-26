use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use bench_note_checker::{
    MixedNotesConfig,
    run_mixed_notes_check_with_measurements,
    setup_mixed_notes_benchmark,
    write_bench_results_to_json,
};

fn main() -> Result<()> {
    // Create a template file for benchmark results.
    let path = Path::new("bin/bench-note-checker/bench-note-checker.json");
    let mut file = File::create(path).context("failed to create file")?;
    file.write_all(b"{}").context("failed to write to file")?;

    // Run benchmarks for different failing note counts.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

    let mut benchmark_results = BTreeMap::new();

    for failing_count in [1, 10, 50, 100] {
        let benchmark_name = format!("mixed_notes_{failing_count}_failing");
        println!("Running benchmark: {benchmark_name}");

        let setup =
            setup_mixed_notes_benchmark(MixedNotesConfig { failing_note_count: failing_count })
                .context("Failed to set up mixed notes benchmark")?;

        let measurements = rt.block_on(async {
            run_mixed_notes_check_with_measurements(&setup)
                .await
                .context("Failed to run mixed notes benchmark")
        })?;

        benchmark_results.insert(benchmark_name, measurements);
    }

    // Write all benchmark results to JSON.
    write_bench_results_to_json(path, benchmark_results)
        .context("Failed to write benchmark results to JSON")?;

    println!("Benchmark results written to: {}", path.display());
    Ok(())
}
