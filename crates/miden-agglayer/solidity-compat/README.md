# Solidity Compatibility Tests

This directory contains Foundry tests for generating test vectors to verify 
that the Miden MMR Frontier implementation is compatible with the Solidity 
`DepositContractBase.sol` from [agglayer-contracts](https://github.com/agglayer/agglayer-contracts).

## Purpose

The Miden implementation of the Keccak-based MMR frontier (`mmr_frontier32_keccak.masm`) 
must produce identical results to the Solidity implementation. These tests generate 
reference test vectors that can be compared against the Rust/MASM implementation.

## Prerequisites

Install [Foundry](https://book.getfoundry.sh/getting-started/installation):

```bash
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

## Running Tests

From the repository root, you can regenerate the test vectors with:

```bash
make generate-solidity-test-vectors
```

Or from this directory:

```bash
# Install dependencies (first time only)
forge install

# Generate test vectors (human-readable)
forge test -vv --match-test test_generateVectors

# Generate canonical zeros (for verifying canonical_zeros.masm)
forge test -vv --match-test test_generateCanonicalZeros

# Generate JSON and save to file (test-vectors/mmr_frontier_vectors.json)
forge test -vv --match-test test_generateVectorsJSON

# Run all tests
forge test -vv
```

### Canonical Zeros

The `test_generateCanonicalZeros` output should match the constants in:
`crates/miden-agglayer/asm/lib/collections/canonical_zeros.masm`

To convert Solidity hex to Miden u32 words:
- Solidity: `0xabcdef...` (64 hex chars = 32 bytes)
- Miden: 8 × u32 values, little-endian within each 4-byte chunk, reversed order

### Test Vectors

The `test_generateVectors` adds leaves `0, 1, 2, ...` (as left-padded 32-byte values)
and outputs the root after each addition.

## Source Files

- `lib/agglayer-contracts/` - Git submodule of [agglayer-contracts](https://github.com/agglayer/agglayer-contracts) @ e468f9b0967334403069aa650d9f1164b1731ebb
- `test/MMRTestVectors.t.sol` - Test vector generation
