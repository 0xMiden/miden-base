# Miden Note Checker Benchmarks

This crate provides performance benchmarks for the `check_notes_consumability` function in the Miden transaction executor.

## Benchmarks

### Mixed Successful and Failing Notes

The primary benchmark (`mixed_successful_and_failing_notes`) tests a scenario with:
- 1 successful note (P2ID note that can be consumed)
- N failing notes (notes that cause division by zero errors)
- 1 more successful note (another P2ID note that can be consumed)

This pattern tests the worst-case scenario for the iterative elimination algorithm:
1. **First iteration**: Executes two notes → first successful note passes, first failing note fails.
2. **Subsequent iterations**: Progressively eliminates failing notes one by one.
3. **Final iteration**: Successfully executes the two valid notes together.

The benchmark varies N (number of failing notes) to measure how performance scales with the number of elimination iterations required.

## Running Benchmarks

To run only the criterion benchmarks:
```bash
cargo bench -p bench-note-checker --bench benchmarks
```

To generate JSON benchmark results:
```bash
cargo run -p bench-note-checker
```
or
```shell
make bench-note-checker
```
