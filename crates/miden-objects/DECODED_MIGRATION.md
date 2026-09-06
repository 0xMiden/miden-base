# Decoded Conversion Migration

## Scope

This experiment starts from `origin/next` at `8195bba1` on branch
`mirko/protobuf-decoded-next`. The foundation introduces `miden-protobuf`,
`miden-protobuf-derive`, and the fixed `DecodeMessage`, `Verify`, `VerifyWith<C>`,
and `BuildUnchecked` traits. Each migration commit covers one wire message.
The protocol domain types are unchanged. The schema now marks
`BlockHeader.next_protocol_config` explicitly optional without changing its field number or encoding.
`StorageValuePatch` now uses a create/update/remove oneof instead of an enum and optional-by-convention
value. This changes its unreleased wire layout; encoders and decoders must be regenerated together.

The initial pass integrated 47 messages. Named enum decoding added another 12, one per commit.
The current inventory covers all 95 Miden message declarations, including nested and empty messages.
The imported `google.protobuf.Empty` descriptor is not counted as a Miden message:

| Status | Messages |
| --- | ---: |
| Generated decoded records with manual `Verify` | 52 |
| Generated decoded records with manual `BuildUnchecked` | 2 |
| Canonical atomic representation adapters | 5 |
| Directly blocked by oneofs | 7 |
| Held for byte-adapter or projection decisions | 3 |
| Blocked by dependencies on those messages | 26 |
| Total | 95 |

The five atoms are `primitives.Word`, `primitives.Felt`, `primitives.MastForest`,
`primitives.ExecutionProof`, and `account.AccountId`. Their existing canonical
representation decoders remain handwritten. This is not 59 fully generated domain conversions.

The generated-record selection is in [build.rs](build.rs); manual construction lives in
[src/decoded](src/decoded). Unmigrated parents retain their existing handwritten conversion
logic, including any inline decoding of children that now also have standalone decoded records.

## Enum Semantics

Decoded enum fields use Prost's named Rust enum, not `i32`. Optional fields become
`Option<Enum>`; repeated fields become `Vec<Enum>`. No separate decoded enum is generated.

Unknown discriminants fail decoding through the existing field wrappers, preserving the
`prost::UnknownEnumValue` source and the complete field/index path. Known values such as
`Unspecified` and `Custom` decode successfully; their domain interpretation belongs in
`Verify` or `BuildUnchecked`. Consequently, a known-but-disallowed version is rejected
after structural decoding, not before missing or malformed payload fields.

Synthetic tests cover singular, optional, and repeated enums, negative discriminants, default
and explicit presence, nested paths, raw Rust identifiers, wire roundtrips, and error sources.
Migrated messages exercise the actual Prost-generated enum types.

## Oneof Gaps

| Message | Unsupported shape |
| --- | --- |
| `account.AccountUpdateDetails` | oneof update |
| `account.StorageSlotPatch` | oneof patch |
| `account.StorageValuePatch` | oneof operation |
| `primitives.SmtLeaf` | oneof leaf |
| `transaction.InputNote` | oneof note |
| `transaction.OutputNote` | oneof note |
| `transaction.TransactionInputs` | oneof version |

Maps and boxed messages are also unsupported by the derive, but do not add another direct
blocker in these schemas. Enum fields no longer block migration.

## Behavioral Gaps

These three messages have field shapes the derive accepts, but migrating them mechanically would
change existing behavior or diagnostics. They were left untouched for review.

| Message | Decision needed |
| --- | --- |
| `primitives.PublicKey` | Decode canonical bytes through a reusable adapter while preserving the generated `encoded` path. |
| `primitives.Signature` | Same byte-adapter requirement as `PublicKey`. |
| `note.Note` | Existing full-note conversion ignores transmitted metadata attachment fields and recomputes them from attachments. |

Both presence gaps are resolved in the schemas. Scheduled upgrades are explicitly optional.
Storage value patches carry a `Word` for Create/Update and `google.protobuf.Empty` for Remove,
which Prost represents as `()`. The existing handwritten conversion rejects an absent operation
and decodes the selected value. Migration of that conversion awaits generator oneof support.
`BlockHeader` still depends on the unmigrated `ValidatorConfig`.

For keys and signatures, deriving a record with a named variant and raw `Vec<u8>` is possible.
However, moving byte parsing to `Verify` would move malformed-byte errors out of generated path
mapping, losing diagnostics such as `validator_config.keys[1].encoded`. The current model has no
generated semantic field-adapter hook; that needs a separate design decision.

For full notes, recursive decoding of the complete `NoteMetadata` record would start requiring
and parsing attachment metadata that the existing conversion deliberately ignores. Standalone
`NoteMetadata` and `NoteHeader` are migrated; the projection used by full `Note` is not.

## Dependency Gaps

These messages need decoded representations of the following unmigrated children. Dependencies
are immediate; follow the table to a oneof or behavioral gap above.

| Message | Unmigrated children |
| --- | --- |
| `account.AccountPatch` | `account.AccountStoragePatch` |
| `account.AccountStoragePatch` | `account.StorageSlotPatch` |
| `account.PartialAccount` | `account.PartialStorage`, `account.PartialVault` |
| `account.PartialStorage` | `account.PartialStorageMap` |
| `account.PartialStorageMap` | `primitives.PartialSmt` |
| `account.PartialVault` | `primitives.PartialSmt` |
| `blockchain.BlockAccountUpdate` | `account.AccountUpdateDetails` |
| `blockchain.BlockBody` | `blockchain.BlockAccountUpdate`, `blockchain.OutputNoteBatch` |
| `blockchain.BlockHeader` | `blockchain.ValidatorConfig` |
| `blockchain.IndexedOutputNote` | `transaction.OutputNote` |
| `blockchain.OutputNoteBatch` | `blockchain.IndexedOutputNote` |
| `blockchain.PartialBlockchain` | `blockchain.BlockHeader` |
| `blockchain.SignedBlock` | `blockchain.BlockBody`, `blockchain.BlockHeader`, `primitives.Signature` |
| `blockchain.ValidatorConfig` | `primitives.PublicKey` |
| `primitives.IndexedSmtLeaf` | `primitives.SmtLeaf` |
| `primitives.PartialSmt` | `primitives.IndexedSmtLeaf` |
| `primitives.SmtOpening` | `primitives.SmtLeaf` |
| `transaction.AuthenticatedInputNote` | `note.Note` |
| `transaction.BatchAccountUpdate` | `account.AccountUpdateDetails` |
| `transaction.InputNotes` | `transaction.InputNote` |
| `transaction.ProposedBatch` | `blockchain.BlockHeader`, `blockchain.PartialBlockchain`, `transaction.ProvenTransaction` |
| `transaction.ProvenBatch` | `transaction.BatchAccountUpdate`, `transaction.OutputNote` |
| `transaction.ProvenTransaction` | `transaction.OutputNote`, `transaction.TxAccountUpdate` |
| `transaction.PublicOutputNote` | `note.Note` |
| `transaction.TransactionInputsV1` | `account.PartialAccount`, `blockchain.BlockHeader`, `blockchain.PartialBlockchain`, `transaction.InputNotes` |
| `transaction.TxAccountUpdate` | `account.AccountUpdateDetails` |

## Construction And Diagnostics

- Field presence, required/optional/repeated message decoding, enum conversion, and decoding
  error paths are generated. Integer narrowing, domain interpretation, duplicates, and cross-field
  checks are deferred to manual construction traits.
- Verification failures are typed semantic errors, not generated wire-path errors. For example,
  duplicate advice keys identify the key rather than a wire index. Existing combined conversion
  APIs preserve those errors in their source chains.
- Nested records remain unverified until their containing constructor explicitly verifies them.
  Infallible nested verifiers still return `Result<_, Infallible>`; fallible parents currently
  unwrap those results with an explicit infallibility justification.
- `InputNoteCommitment` implements only `BuildUnchecked`: present headers are verified, but
  nullifier/header consistency and inclusion authentication remain external responsibilities.
- `TransactionHeader` also implements only `BuildUnchecked`, because it builds unchecked input
  commitments. It still checks note uniqueness, note-header invariants, and the transmitted
  transaction ID. Neither type is presented as fully verified.
- No production `VerifyWith` implementation has been added yet. Context-sensitive candidates such
  as `BlockHeader` and `ProposedBatch` remain blocked; synthetic tests cover the trait.
- `TransactionScript::from_parts` returns a type from a private protocol module. Its verification
  error boxes that source to preserve it without changing the protocol API.
- `AccountWitness` verifies constructor invariants, not authentication against a trusted root.
  Its unchecked constructor is `pub(super)`, so no unchecked capability is exposed here.
- Compatibility conversions for borrowed messages clone into the owned decoding model.
  Existing infallible `From` conversions for `BlockNumber` and `FeeParameters` remain infallible.

The derive supports named enums; the traits and construction model remain unchanged. The presence
schema fixes do not add generator oneof support or field adapters.
