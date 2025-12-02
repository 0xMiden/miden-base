#!/bin/bash

set -euo pipefail

# Script to check all feature combinations compile without warnings
# This script ensures that warnings are treated as errors for CI

echo "Checking all feature combinations with cargo-hack..."

# Set environment variables to treat warnings as errors
export RUSTFLAGS="-D warnings"

# Enable file generation in the `src` directory for miden-lib build scripts
export BUILD_GENERATED_FILES_IN_SRC=1

# Run cargo-hack with comprehensive feature checking
# Focus on library packages that have significant feature matrices
for package in miden-objects miden-tx miden-testing miden-block-prover miden-tx-batch-prover; do
    echo "Checking package: $package"
    cargo hack check -p "$package" --each-feature --all-targets
done

# For miden-lib, we need to be more careful due to build dependencies and complex features
echo "Checking package: miden-lib"
# Just check with all features for now to ensure basic compilation works
cargo check -p miden-lib --all-features --all-targets

echo "All feature combinations compiled successfully!"