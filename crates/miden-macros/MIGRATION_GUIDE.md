// Example: How to refactor existing Word wrapper types to use the WordWrapper macro
//
// This file demonstrates how types like NoteId, TransactionId, etc. can be simplified
// by using the WordWrapper derive macro.

// BEFORE: Manual implementation (from note_id.rs)
//
// ```rust
// use alloc::string::String;
// use core::fmt::Display;
// 
// use super::{Felt, Hasher, NoteDetails, Word};
// use crate::WordError;
// use crate::utils::serde::{
//     ByteReader,
//     ByteWriter,
//     Deserializable,
//     DeserializationError,
//     Serializable,
// };
//
// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct NoteId(Word);
//
// impl NoteId {
//     pub fn new(recipient: Word, asset_commitment: Word) -> Self {
//         Self(Hasher::merge(&[recipient, asset_commitment]))
//     }
//
//     pub fn as_elements(&self) -> &[Felt] {
//         self.0.as_elements()
//     }
//
//     pub fn as_bytes(&self) -> [u8; 32] {
//         self.0.as_bytes()
//     }
//
//     pub fn to_hex(&self) -> String {
//         self.0.to_hex()
//     }
//
//     pub fn as_word(&self) -> Word {
//         self.0
//     }
// }
//
// impl From<Word> for NoteId {
//     fn from(digest: Word) -> Self {
//         Self(digest)
//     }
// }
//
// impl From<NoteId> for Word {
//     fn from(id: NoteId) -> Self {
//         id.0
//     }
// }
//
// impl From<&NoteId> for Word {
//     fn from(id: &NoteId) -> Self {
//         id.0
//     }
// }
//
// impl From<NoteId> for [u8; 32] {
//     fn from(id: NoteId) -> Self {
//         id.0.into()
//     }
// }
//
// impl From<&NoteId> for [u8; 32] {
//     fn from(id: &NoteId) -> Self {
//         id.0.into()
//     }
// }
// ```

// AFTER: Using WordWrapper macro
//
// ```rust
// use alloc::string::String;
// use core::fmt::Display;
// 
// use super::{Felt, Hasher, NoteDetails, Word};
// use crate::WordError;
// use crate::utils::serde::{
//     ByteReader,
//     ByteWriter,
//     Deserializable,
//     DeserializationError,
//     Serializable,
// };
// use miden_macros::WordWrapper;
//
// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, WordWrapper)]
// pub struct NoteId(Word);
//
// impl NoteId {
//     pub fn new(recipient: Word, asset_commitment: Word) -> Self {
//         Self(Hasher::merge(&[recipient, asset_commitment]))
//     }
// }
//
// // All the accessor methods and From conversions are automatically generated!
// ```
//
// Benefits:
// - Removes ~45 lines of boilerplate code
// - Ensures consistency across all Word wrapper types
// - Easier to maintain and update
// - Reduces the chance of copy-paste errors
//
// The macro generates exactly the same implementations as the manual code,
// so it's a drop-in replacement with no behavior changes.

// The following types in miden-objects could benefit from this macro:
// - NoteId (crates/miden-objects/src/note/note_id.rs)
// - TransactionId (crates/miden-objects/src/transaction/transaction_id.rs)
// - Nullifier (crates/miden-objects/src/note/nullifier.rs)
// - BatchId (crates/miden-objects/src/batch/batch_id.rs)
// - PublicKeyCommitment (crates/miden-objects/src/account/auth.rs)
// - NonFungibleAsset (crates/miden-objects/src/asset/nonfungible.rs)
// - AssetVaultKey (crates/miden-objects/src/asset/vault/vault_key.rs)

// To apply the macro:
// 1. Add `miden-macros` to the dependencies in crates/miden-objects/Cargo.toml:
//    ```toml
//    [dependencies]
//    miden-macros = { workspace = true }
//    ```
//
// 2. Add `WordWrapper` to the imports at the top of each file
//
// 3. Add `WordWrapper` to the derive list
//
// 4. Remove the manual implementations of:
//    - as_elements()
//    - as_bytes()
//    - to_hex()
//    - as_word()
//    - From<Word>
//    - From<T> for Word
//    - From<&T> for Word
//    - From<T> for [u8; 32]
//    - From<&T> for [u8; 32]
//
// 5. Keep any custom constructors (like `new()`) and other custom methods
//
// 6. Keep Display, Serializable, and other trait implementations
