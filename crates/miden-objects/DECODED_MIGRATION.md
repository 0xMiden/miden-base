# Decoded Conversion Migration

## Scope

This experiment starts from `origin/next` at `8195bba1` on branch
`mirko/protobuf-decoded-next`. The foundation introduces `miden-protobuf`,
`miden-protobuf-derive`, and the fixed `DecodeMessage`, `Verify`, `VerifyWith<C>`,
and `BuildUnchecked` traits. Each subsequent migration commit covers one wire message.
The schemas and protocol domain types are unchanged.

A descriptor-based audit of all 95 message declarations, including nested and empty messages,
found:

| Status | Messages |
| --- | ---: |
| Generated decoded records with manual `Verify` | 42 |
| Canonical atomic representation adapters | 5 |
| Directly blocked by enums or oneofs | 16 |
| Blocked by dependencies on those messages | 32 |
| Total | 95 |

All 47 messages supported by the current model are integrated. The five atoms are
`primitives.Word`, `primitives.Felt`, `primitives.MastForest`,
`primitives.ExecutionProof`, and `account.AccountId`. Their existing canonical
representation decoders remain handwritten. This is not 47 fully generated domain conversions.

The generated-record selection is in [build.rs](build.rs); manual construction lives in
[src/decoded](src/decoded). Unmigrated parents retain their existing handwritten conversion
logic, including any inline decoding of children that now also have standalone decoded records.

## Direct Gaps

There are ten messages with enum fields and six with oneofs. The derive deliberately rejects
both rather than selecting a domain interpretation or silently retaining unchecked wire values.

| Message | Unsupported shape |
| --- | --- |
| `account.AccountHeader` | enum version (account.AccountVersion) |
| `account.AccountPatch` | enum version (account.AccountPatchVersion) |
| `account.AccountStorageHeader.StorageSlot` | enum slot_type (account.StorageSlotType) |
| `account.AccountUpdateDetails` | oneof update |
| `account.StorageMapPatch` | enum operation (account.StoragePatchOperation) |
| `account.StorageSlotPatch` | oneof patch |
| `account.StorageValuePatch` | enum operation (account.StoragePatchOperation) |
| `asset.AssetId` | enum composition (asset.AssetComposition), enum version (asset.AssetVersion) |
| `blockchain.BlockHeader` | enum version (blockchain.BlockVersion) |
| `note.NoteMetadata` | enum note_type (note.NoteType), enum version (note.NoteVersion) |
| `primitives.PublicKey` | enum variant (primitives.PublicKeyVariant) |
| `primitives.Signature` | enum variant (primitives.SignatureVariant) |
| `primitives.SmtLeaf` | oneof leaf |
| `transaction.InputNote` | oneof note |
| `transaction.OutputNote` | oneof note |
| `transaction.TransactionInputs` | oneof version |

Maps and boxed messages are also unsupported by the derive, but they do not add another
direct blocker in these schemas.

## Dependency Gaps

These messages have supported local field shapes, but need decoded representations of the
following unmigrated children. Dependencies are immediate; follow the table to a direct gap
above.

| Message | Unmigrated children |
| --- | --- |
| `account.AccountStorageHeader` | `account.AccountStorageHeader.StorageSlot` |
| `account.AccountStoragePatch` | `account.StorageSlotPatch` |
| `account.PartialAccount` | `account.PartialStorage`, `account.PartialVault` |
| `account.PartialStorage` | `account.AccountStorageHeader`, `account.PartialStorageMap` |
| `account.PartialStorageMap` | `primitives.PartialSmt` |
| `account.PartialVault` | `primitives.PartialSmt` |
| `asset.Asset` | `asset.AssetId` |
| `blockchain.BlockAccountUpdate` | `account.AccountUpdateDetails` |
| `blockchain.BlockBody` | `blockchain.BlockAccountUpdate`, `blockchain.OutputNoteBatch`, `transaction.TransactionHeader` |
| `blockchain.IndexedOutputNote` | `transaction.OutputNote` |
| `blockchain.OutputNoteBatch` | `blockchain.IndexedOutputNote` |
| `blockchain.PartialBlockchain` | `blockchain.BlockHeader` |
| `blockchain.SignedBlock` | `blockchain.BlockBody`, `blockchain.BlockHeader`, `primitives.Signature` |
| `blockchain.ValidatorConfig` | `primitives.PublicKey` |
| `note.Note` | `note.NoteDetails`, `note.NoteMetadata` |
| `note.NoteDetails` | `asset.Asset` |
| `note.NoteHeader` | `note.NoteMetadata` |
| `primitives.IndexedSmtLeaf` | `primitives.SmtLeaf` |
| `primitives.PartialSmt` | `primitives.IndexedSmtLeaf` |
| `primitives.SmtOpening` | `primitives.SmtLeaf` |
| `transaction.AuthenticatedInputNote` | `note.Note` |
| `transaction.BatchAccountUpdate` | `account.AccountUpdateDetails` |
| `transaction.InputNoteCommitment` | `note.NoteHeader` |
| `transaction.InputNotes` | `transaction.InputNote` |
| `transaction.PrivateOutputNote` | `note.NoteHeader` |
| `transaction.ProposedBatch` | `blockchain.BlockHeader`, `blockchain.PartialBlockchain`, `transaction.ProvenTransaction` |
| `transaction.ProvenBatch` | `transaction.BatchAccountUpdate`, `transaction.InputNoteCommitment`, `transaction.OutputNote`, `transaction.TransactionHeader` |
| `transaction.ProvenTransaction` | `transaction.InputNoteCommitment`, `transaction.OutputNote`, `transaction.TxAccountUpdate` |
| `transaction.PublicOutputNote` | `note.Note` |
| `transaction.TransactionHeader` | `note.NoteHeader`, `transaction.InputNoteCommitment` |
| `transaction.TransactionInputsV1` | `account.PartialAccount`, `blockchain.BlockHeader`, `blockchain.PartialBlockchain`, `transaction.InputNotes` |
| `transaction.TxAccountUpdate` | `account.AccountUpdateDetails` |

## Review Points

- Field presence, required/optional/repeated message decoding, and decoding error paths are
  generated. Integer narrowing, domain interpretation, duplicates, and cross-field checks are
  deferred to manual `Verify` implementations.
- Verification failures are typed semantic errors, not generated wire-path errors. For example,
  duplicate advice keys identify the key rather than a wire index. Existing combined conversion
  APIs preserve those errors in their source chains.
- Nested records remain unverified until their containing verifier explicitly verifies them.
  Infallible nested verifiers still return `Result<_, Infallible>`; fallible parents currently
  unwrap those results with an explicit infallibility justification.
- `TransactionScript::from_parts` returns a type from a private protocol module. Its verification
  error boxes that source to preserve it without changing the protocol API.
- The context-sensitive candidates, including `BlockHeader` and `ProposedBatch`, are blocked
  by the shapes above. No production `VerifyWith` or `BuildUnchecked` implementation has been
  added yet; their independent opt-in behavior is covered by synthetic tests.
- `AccountWitness` supports context-free verification of its constructor invariants, not
  cryptographic authentication against a trusted root. Its unchecked constructor is
  `pub(super)`, so no unchecked construction capability is exposed here.
- Compatibility conversions for borrowed messages clone into the owned decoding model.
  Existing infallible `From` conversions for `BlockNumber` and `FeeParameters` remain infallible.

Enum decoding is the next useful design question: it blocks more messages directly than oneofs
and prevents exercising contextual verification on real block headers. No trait/model extension
was made during this migration pass.
