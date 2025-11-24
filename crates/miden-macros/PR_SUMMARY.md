# WordWrapper Proc Macro Implementation

## Summary

This PR introduces a new proc-macro crate `miden-macros` that provides a `WordWrapper` derive macro to reduce boilerplate code for types that wrap a `Word`.

## Problem

The Miden codebase has many types that wrap a `Word` in a tuple struct pattern (e.g., `pub struct NoteId(Word)`). Each of these types manually implements the same set of accessor methods and `From` trait conversions, resulting in approximately 45-50 lines of repetitive code per type.

Currently affected types include:
- `NoteId`
- `TransactionId`
- `Nullifier`
- `BatchId`
- `PublicKeyCommitment`
- `NonFungibleAsset`
- `AssetVaultKey`

## Solution

The `WordWrapper` derive macro automatically generates:

1. **Accessor Methods:**
   - `as_elements(&self) -> &[Felt]`
   - `as_bytes(&self) -> [u8; 32]`
   - `to_hex(&self) -> String`
   - `as_word(&self) -> Word`

2. **Conversion Traits:**
   - `From<Word> for T`
   - `From<T> for Word`
   - `From<&T> for Word`
   - `From<T> for [u8; 32]`
   - `From<&T> for [u8; 32]`

## Changes

### New Crate: `crates/miden-macros`

- **Cargo.toml**: Proc-macro crate configuration
- **src/lib.rs**: Implementation of the `WordWrapper` derive macro
- **tests/integration_test.rs**: Comprehensive test suite
- **README.md**: Documentation and usage examples
- **MIGRATION_GUIDE.md**: Guide for refactoring existing types

### Workspace Changes

- Added `miden-macros` to workspace members in root `Cargo.toml`
- Added `miden-macros` to workspace dependencies

## Usage Example

### Before (Manual Implementation)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteId(Word);

impl NoteId {
    pub fn as_elements(&self) -> &[Felt] {
        self.0.as_elements()
    }

    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.as_bytes()
    }

    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn as_word(&self) -> Word {
        self.0
    }
}

impl From<Word> for NoteId {
    fn from(digest: Word) -> Self {
        Self(digest)
    }
}

// ... plus 3 more From implementations
```

### After (Using WordWrapper)

```rust
use miden_macros::WordWrapper;

#[derive(Debug, Clone, Copy, PartialEq, Eq, WordWrapper)]
pub struct NoteId(Word);

// All accessor methods and conversions are automatically generated!
```

## Benefits

1. **Reduces Boilerplate**: Eliminates ~45 lines of repetitive code per type
2. **Consistency**: Ensures all Word wrapper types have identical implementations
3. **Maintainability**: Changes to the pattern only need to be made in one place
4. **Type Safety**: Compile-time validation that the macro is only applied to appropriate types
5. **Documentation**: The macro includes comprehensive documentation

## Testing

The implementation includes:
- Unit tests for the macro's validation logic
- Integration tests demonstrating usage with actual `Word` types
- Tests for all generated methods and conversions

## Future Work

The existing Word wrapper types in `miden-objects` can be refactored to use this macro in a follow-up PR. The `MIGRATION_GUIDE.md` provides detailed instructions for this refactoring.

## References

- Related to issue: https://github.com/0xMiden/crypto/issues/430
