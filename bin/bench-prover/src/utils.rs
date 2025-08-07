use std::env;
use std::fs::{self};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};

// replicating this function from the criterion lib:
// https://github.com/bheisler/criterion.rs/blob/ccccbcc15237233af22af4c76751a7aa184609b3/src/lib.rs#L366.
pub fn cargo_target_directory() -> Option<PathBuf> {
    #[derive(Deserialize)]
    struct Metadata {
        target_directory: PathBuf,
    }

    env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).or_else(|| {
        let output = Command::new(env::var_os("CARGO")?)
            .args(["metadata", "--format-version", "1"])
            .output()
            .ok()?;
        let metadata: Metadata = serde_json::from_slice(&output.stdout).ok()?;
        Some(metadata.target_directory)
    })
}
const NANOS_PER_SEC: f64 = 1_000_000_000.0;

/// Processes Criterion benchmark output files (benchmark.json, estimates.json, sample.json)
/// and extracts relevant performance metrics into a single JSON structure.
/// Converts nanosecond measurements to seconds and includes mean, confidence intervals,
/// standard deviation, and individual sample times.
pub fn process_benchmark_data(benchmark_path: &Path) -> Result<Value> {
    let mut benchmark_data = json!({});

    // Process benchmark.json
    let benchmark_file = benchmark_path.join("benchmark.json");
    let benchmark_content = fs::read_to_string(&benchmark_file).with_context(|| {
        format!("Failed to read benchmark file at {}", benchmark_file.display())
    })?;

    let json: Value = serde_json::from_str(&benchmark_content).with_context(|| {
        format!("Failed to parse benchmark.json at {}", benchmark_file.display())
    })?;
    benchmark_data["id"] = json["full_id"].clone();

    // Process estimates.json
    let estimates_file = benchmark_path.join("estimates.json");
    let estimates_content = fs::read_to_string(&estimates_file).with_context(|| {
        format!("Failed to read estimates file at {}", estimates_file.display())
    })?;

    let json: Value = serde_json::from_str(&estimates_content).with_context(|| {
        format!("Failed to parse estimates.json at {}", estimates_file.display())
    })?;

    // Extract metrics
    let mean = json["mean"]["point_estimate"]
        .as_f64()
        .with_context(|| "Missing or invalid mean point estimate in estimates.json")?;
    benchmark_data["mean_sec"] = json!(mean / NANOS_PER_SEC);

    let lower = json["mean"]["confidence_interval"]["lower_bound"]
        .as_f64()
        .with_context(|| "Missing or invalid lower bound in estimates.json")?;
    benchmark_data["mean_lower_bound_sec"] = json!(lower / NANOS_PER_SEC);

    let upper = json["mean"]["confidence_interval"]["upper_bound"]
        .as_f64()
        .with_context(|| "Missing or invalid upper bound in estimates.json")?;
    benchmark_data["mean_upper_bound_sec"] = json!(upper / NANOS_PER_SEC);

    let std_dev = json["std_dev"]["point_estimate"]
        .as_f64()
        .with_context(|| "Missing or invalid std_dev point estimate in estimates.json")?;
    benchmark_data["std_dev_sec"] = json!(std_dev / NANOS_PER_SEC);

    // Process sample.json
    let sample_file = benchmark_path.join("sample.json");
    let sample_content = fs::read_to_string(&sample_file)
        .with_context(|| format!("Failed to read sample file at {}", sample_file.display()))?;

    let json: Value = serde_json::from_str(&sample_content)
        .with_context(|| format!("Failed to parse sample.json at {}", sample_file.display()))?;

    let times_array = json["times"]
        .as_array()
        .with_context(|| "Missing or invalid time values in sample.json")?;

    let times_sec: Vec<f64> = times_array
        .iter()
        .map(|v| v.as_f64().ok_or_else(|| anyhow::anyhow!("Invalid time values")))
        .collect::<Result<Vec<f64>>>()?
        .into_iter()
        .map(|t| t / NANOS_PER_SEC)
        .collect();

    benchmark_data["times_sec"] = json!(times_sec);

    // Do the same for trials
    let trials_array = json["iters"]
        .as_array()
        .with_context(|| "Missing or invalid iters array in sample.json")?;
    benchmark_data["trial_count"] = json!(trials_array.len());

    Ok(benchmark_data)
}

// Update signature for save_json_to_file to use anyhow
pub fn save_json_to_file(data: &Value, file_path: &Path) -> Result<()> {
    let mut file = fs::File::create(file_path)
        .with_context(|| format!("Failed to create file at {}", file_path.display()))?;
    let json_string =
        serde_json::to_string_pretty(data).context("Failed to convert data to JSON string")?;
    file.write_all(json_string.as_bytes())
        .with_context(|| format!("Failed to write JSON to file at {}", file_path.display()))?;
    Ok(())
}
