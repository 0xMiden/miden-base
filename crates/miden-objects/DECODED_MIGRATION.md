
# Decoded Conversion Migration

## Coverage

This experiment starts from `origin/next` at `8195bba1` on branch
`mirko/protobuf-decoded-next`. The original construction traits remain unchanged:
`DecodeMessage`, `Verify`, `VerifyWith<C>`, and `BuildUnchecked`.
Each migration commit covers one wire message.

All 97 Miden message declarations, including nested and empty messages, are integrated.
The imported `google.protobuf.Empty` is not counted as a Miden message.

| Primary construction capability | Messages |
| --- | ---: |
| Generated records with manual `Verify` | 82 |
| Generated records with manual `BuildUnchecked` | 10 |
| Generated record with contextual verification only | 1 |
| Canonical atomic representation adapters | 4 |
| Unmigrated messages | 0 |
| Total | 97 |

Two unchecked records also implement `VerifyWith`. The four atoms are
`primitives.Word`, `primitives.Felt`, `primitives.MastForest`,
and `primitives.ExecutionProof`. Their existing canonical
representation parsing and leaf diagnostics remain handwritten. This is not 97 fully
generated domain conversions.

Generated-record selection and the two byte adapter attributes live in [build.rs](build.rs).
Handwritten construction lives in [src/decoded](src/decoded). Combined protobuf-to-domain
`TryFrom`/`From` conversions, including borrowed wrappers and batch decoding helpers, have been
removed. Callers must use `decode_fields()` and explicitly choose `verify()`, `verify_with(context)`,
or `build_unchecked()`. Generated `TryFrom` implementations target decoded records, not domain
objects; atomic representation adapters retain their own `TryFrom` implementations.
The old required-field helper and handwritten proven-batch parts struct have also been removed.

## Structural Decoding

- Message cardinality, presence, enum conversion, and field/index paths are generated.
- Explicitly optional message presence is injected from schema descriptors.
- Enum fields use Prost's named enum, not integers. Unknown discriminants fail decoding with
  a preserved `prost::UnknownEnumValue` source. Known-but-disallowed values fail construction.
- Each Prost oneof generates a corresponding decoded enum. Variant wire names come from
  descriptors, not case conversion or handwritten path annotations.
- Oneofs are required by default; `#[proto_decode(optional)]` supports consumers that permit
  absence. `google.protobuf.Empty` oneof payloads retain Prost's `()` representation.
- `#[proto_decode(bytes = Adapter)]` selects an ordinary `TryFrom` representation adapter
  for bytes, including optional/repeated fields and oneof payloads. Paths remain generated.
- Keys and signatures use the reusable `Canonical<T>` adapter. It deserializes and requires
  reserialization to match, rejecting trailing or noncanonical bytes. It does not authenticate.
- Maps and boxed messages remain unsupported, but neither blocks these schemas.

No domain target, constructor, ordered-field, or variant-mapping DSL was added.
Domain oneof conversion is an ordinary Rust match. For example, an SMT error can carry
`leaf.multiple.entries[0].key`, and a malformed validator key carries
`validator_config.keys[1].key.ecdsa_k256_keccak`.

## Construction And Trust

Verification errors are typed semantic errors, not generated wire-path errors. Duplicate
identifiers identify the conflicting value; cross-field checks need not have one wire location.
Errors remain available in source chains, including errors boxed because their protocol modules
do not publicly export the error types.

Nested records remain unverified until their parent explicitly constructs them.
The following records expose `BuildUnchecked`:

| Record | Checks still external |
| --- | --- |
| `InputNoteCommitment` | Nullifier/header consistency and inclusion authentication. |
| `TransactionHeader` | Input commitment authentication and original note ordering. |
| `BlockHeader` | Parent linkage, signatures, and protocol transition validity. |
| `PartialBlockchain` | Header authentication and trust in the supplied root; MMR reconstruction and membership are checked. |
| `BlockBody` | Transaction ordering and input commitment authentication; body invariants are checked. |
| `SignedBlock` | Parent authentication; header/body consistency is checked. |
| `ProvenTransaction` | Execution-proof validity and input authentication; constructor invariants are checked. |
| `ProvenBatch` | Proof validity, complete note aggregation, and transaction ordering; local batch invariants are checked. |
| `TransactionInputsV1` | Trust in supplied headers/chain; input consistency and note inclusion are checked. |
| `TransactionInputs` | Same as the selected version. |

Contextual capabilities:

- `SignedBlock::verify_with(&trusted_parent)` checks self-consistency and authenticates against
  the parent's committed validator set. The parent must already be trusted. This does not
  re-execute transactions or validate account/nullifier state transitions.
- `ProposedBatch::verify_with(security_level)` checks batch invariants and verifies transaction
  proofs at the requested security level. It does not independently authenticate the chain.
  No production unchecked constructor exists, so no `BuildUnchecked` is exposed.
- `ProvenBatch::verify_with(&proposal)` additionally checks every duplicated proposal field.
  The proposal must already be verified. The batch execution proof still requires verification
  at the service boundary.

`Verify` on signature/key records only constructs their canonical representation; it does
not assert signature authenticity. Similarly, input-note and account-witness construction checks
their available invariants, not authentication against an external trusted root.

The protocol domain types were not changed to widen private APIs.
`BlockHeader` parent validation is private and includes signatures, so contextual authentication
uses the public `SignedBlock::validate` API rather than duplicating that logic.

## Unreleased Schema Changes

- `BlockHeader.next_protocol_config` is explicitly optional. Its tag and encoding are unchanged.
- `StorageValuePatch` is a create/update/remove oneof, with a Word for create/update and
  `google.protobuf.Empty` for remove.
- `PublicKey` and `Signature` use algorithm-specific byte oneofs instead of an enum plus bytes.
- `AccountId` uses a version oneof instead of canonical bytes. Its `AccountIdV1` payload
  carries suffix/prefix Felts. Structural decoding is generated; `Verify` checks the account-ID
  bit constraints with the protocol's checked constructor before wrapping the V1 variant.
- Full `Note` carries the new `PartialNoteMetadata`, while standalone `NoteMetadata` and
  `NoteHeader` retain full attachment metadata. Full notes derive attachment headers and
  commitments from attachment content rather than silently ignoring redundant fields.

Storage patches, account IDs, and cryptographic oneofs change the wire layout. These schemas are
unreleased; regenerate consumers together. Existing domain encoders remain handwritten.

## Validation

Tests cover generated presence and nested paths, named enum sources, all oneof payload shapes
used here, optional oneofs, byte adapters, canonical key/signature bytes, schema descriptors,
domain roundtrips, malformed inputs, duplicates, trusted-parent authentication, deferred proof
verification, and proposal agreement. Clippy and no-default-features checks cover all three crates.
