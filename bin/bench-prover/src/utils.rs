use std::{
    env,
    fs::{self},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use serde_json::{Result as JsonResult, Value, json};

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

pub fn process_benchmark_data(benchmark_path: &Path) -> JsonResult<Value> {
    let mut benchmark_data = json!({});

    // Process benchmark.json
    let benchmark_content =
        fs::read_to_string(benchmark_path.join("benchmark.json")).unwrap_or_default();
    if !benchmark_content.is_empty() {
        let json: Value = serde_json::from_str(&benchmark_content).unwrap_or_default();
        benchmark_data["id"] = json["full_id"].clone();
    }

    // Process estimates.json
    let estimates_content =
        fs::read_to_string(benchmark_path.join("estimates.json")).unwrap_or_default();
    if !estimates_content.is_empty() {
        let json: Value = serde_json::from_str(&estimates_content).unwrap_or_default();

        // Extract metrics directly with unwrap
        let mean = json["mean"]["point_estimate"].as_f64().unwrap_or(0.0);
        benchmark_data["mean_sec"] = json!(mean / 1_000_000_000.0);

        let lower = json["mean"]["confidence_interval"]["lower_bound"].as_f64().unwrap_or(0.0);
        benchmark_data["mean_lower_bound_sec"] = json!(lower / 1_000_000_000.0);

        let upper = json["mean"]["confidence_interval"]["upper_bound"].as_f64().unwrap_or(0.0);
        benchmark_data["mean_upper_bound_sec"] = json!(upper / 1_000_000_000.0);

        let std_dev = json["std_dev"]["point_estimate"].as_f64().unwrap_or(0.0);
        benchmark_data["std_dev_sec"] = json!(std_dev / 1_000_000_000.0);
    }

    let sample_content = fs::read_to_string(benchmark_path.join("sample.json")).unwrap_or_default();
    if !sample_content.is_empty() {
        let json: Value = serde_json::from_str(&sample_content).unwrap_or_default();

        let empty_vec = Vec::new();
        let times_array = json["times"].as_array().unwrap_or(&empty_vec);

        let times_sec: Vec<f64> = times_array
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) / 1_000_000_000.0)
            .collect();

        benchmark_data["times_sec"] = json!(times_sec);

        // Do the same for trials
        let trials_array = json["iters"].as_array().unwrap_or(&empty_vec);
        benchmark_data["trial_count"] = json!(trials_array.len());
    }

    Ok(benchmark_data)
}

pub fn save_json_to_file(data: &Value, file_path: &Path) -> std::io::Result<()> {
    let mut file = fs::File::create(file_path)?;
    let json_string = serde_json::to_string_pretty(data)?;
    file.write_all(json_string.as_bytes())?;
    Ok(())
}
