# Miden Prover Benchmarking

This document describes how to run and analyze benchmarks for the Miden prover.

## Running Benchmarks

You can run the benchmarks in two ways:

### Option 1: Using Make (from miden-base directory)

```bash
make bench-prover
```

### Option 2: Running Directly (from bench-prover directory)

```bash
# Run the benchmarks
cargo bench

# Process the results
cargo run
```

## How It Works

1. `cargo bench` uses [Criterion.rs](https://github.com/bheisler/criterion.rs) to run performance benchmarks
2. By default, Criterion stores raw benchmark results in `target/criterion/`
3. `cargo run` parses these results and creates a consolidated summary in `consolidated_benchmarks.json`

## Viewing Results

### HTML Reports

Criterion automatically generates HTML reports with its built-in reporting feature. After running the benchmarks, you can find these reports in the Criterion directory by default under `target/criterion/{BENCHMARK_GROUP}/index.html`


### Consolidated JSON Summary

The `consolidated_benchmarks.json` file contains a summary of all proving benchmarks in a structured format:

Example `consolidated_benchmarks.json` structure:
```json
{
  "prove_consume_note_with_new_account": {
    "id": "miden_proving/prove_consume_note_with_new_account",
    // average time per trial in seconds
    "mean_sec": 2.9489723874,
    // lower bound of 95% confidence interval for mean
    "mean_lower_bound_sec": 2.924891996,
    // upper bound of 95% confidence interval for mean
    "mean_upper_bound_sec": 2.9777331873,
    // standard deviation of time per trial
    "std_dev_sec": 0.04551027448900068,
    // denotes time for each trial
    "times_sec": [
      2.98336025,
      3.051340166,
      2.972870583,
      2.943372125,
      2.923954958,
      2.939220542,
      2.945244416,
      2.890069959,
      2.9041745,
      2.936116375
    ],
    // total number of trials benchmark completed
    "trial_count": 10
  },
  "prove_consume_multiple_notes": {
    "id": "miden_proving/prove_consume_multiple_notes",
    "mean_sec": 2.0523832292,
    "mean_lower_bound_sec": 2.0268005916,
    "mean_upper_bound_sec": 2.0808349876,
    "std_dev_sec": 0.04648980316499867,
    "times_sec": [
      2.118166333,
      2.121326834,
      2.112017625,
      2.028088083,
      2.014474333,
      2.000334667,
      2.018519542,
      2.024895417,
      2.043308542,
      2.042700916
    ],
    "trial_count": 10
  }
}
```
