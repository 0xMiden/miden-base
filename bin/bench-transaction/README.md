# Miden Transaction Benchmarking

This document describes how to run the benchmarks for the Miden transaction.

Benchmarks consist of two parts:
- Benchmarking the transaction execution.

  For each transaction, data is collected on the number of cycles required to complete:
  - Prologue
  - All notes processing
  - Each note execution
  - Transaction script processing
  - Epilogue
  - After compute_fee (The number of cycles the epilogue took to execute after compute_fee determined the cycle count)
  
  Results of this benchmark will be stored in the [`bin/bench-tx/bench-tx.json`](bench-tx.json) file.
- Benchmarking the transaction proving. 

  This type uses the [Criterion.rs](https://github.com/bheisler/criterion.rs) to collect the data how much time it took to prove the transaction. Results of this benchmark will be printed to the terminal. 

## Running Benchmarks

You can run the benchmarks in two ways:

### Option 1: Using Make (from miden-base directory)

```bash
make bench-tx
```

This command will run both the execution and the proving benchmarks.

### Option 2: Running each benchmark individually (from miden-base directory)

```bash
# Run the execution benchmarks
cargo run --bin bench-transaction

# Run the proving benchmarks
cargo bench --bin bench-transaction --bench proving_benchmarks
```

## License

This project is [MIT licensed](../../LICENSE).