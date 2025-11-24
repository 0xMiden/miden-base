# Example: Refactoring NoteId to use WordWrapper

This patch demonstrates how to refactor `NoteId` to use the `WordWrapper` macro.

## Step 1: Add dependency

In `crates/miden-objects/Cargo.toml`, add to the `[dependencies]` section:

```toml
miden-macros = { workspace = true }
```

## Step 2: Update imports

In `crates/miden-objects/src/note/note_id.rs`, add to the imports:

```rust
use miden_macros::WordWrapper;
```

## Step 3: Add derive macro

Change the struct definition from:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(Word);
```

To:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, WordWrapper)]
pub struct NoteId(Word);
```

## Step 4: Remove redundant implementations

Delete the following manually implemented methods and traits:

### Delete these methods from the `impl NoteId` block:

```rust
// DELETE:
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
```

### Delete these trait implementations:

```rust
// DELETE:
impl From<Word> for NoteId {
    fn from(digest: Word) -> Self {
        Self(digest)
    }
}

impl From<NoteId> for Word {
    fn from(id: NoteId) -> Self {
        id.0
    }
}

impl From<&NoteId> for Word {
    fn from(id: &NoteId) -> Self {
        id.0
    }
}

impl From<NoteId> for [u8; 32] {
    fn from(id: NoteId) -> Self {
        id.0.into()
    }
}

impl From<&NoteId> for [u8; 32] {
    fn from(id: &NoteId) -> Self {
        id.0.into()
    }
}
```

## Step 5: Keep custom logic

Keep all custom constructors and other methods. For example, keep:

```rust
impl NoteId {
    /// Returns a new [NoteId] instantiated from the provided note components.
    pub fn new(recipient: Word, asset_commitment: Word) -> Self {
        Self(Hasher::merge(&[recipient, asset_commitment]))
    }
}

impl NoteId {
    /// Attempts to convert from a hexadecimal string to [NoteId].
    pub fn try_from_hex(hex_value: &str) -> Result<NoteId, WordError> {
        Word::try_from(hex_value).map(NoteId::from)
    }
}
```

Also keep all the trait implementations for `Display`, `From<&NoteDetails>`, `Serializable`, `Deserializable`, etc.

## Result

The refactored file will have ~45 fewer lines of boilerplate code while maintaining identical functionality. All tests should continue to pass without modification.

## Apply to other types

The same refactoring can be applied to:
- `TransactionId` (crates/miden-objects/src/transaction/transaction_id.rs)
- `Nullifier` (crates/miden-objects/src/note/nullifier.rs)
- `BatchId` (crates/miden-objects/src/batch/batch_id.rs)
- `PublicKeyCommitment` (crates/miden-objects/src/account/auth.rs)
- `NonFungibleAsset` (crates/miden-objects/src/asset/nonfungible.rs)
- `AssetVaultKey` (crates/miden-objects/src/asset/vault/vault_key.rs)

Each will follow the same pattern: add the derive, remove the generated methods.
