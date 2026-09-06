# Miden Objects

Canonical Protobuf representations for values exchanged between Miden clients and nodes.

This crate owns the transport representation and conversions for protocol objects. It does not
define protocol commitments, replace the protocol's native serialization, or define RPC services.
Generated messages are exposed under `miden_objects::proto`.

The crate supports `no_std` consumers when default features are disabled.

RPC crates can use `FILE_DESCRIPTOR_SET` as the import descriptor and apply every entry in
`EXTERN_PATHS` with `prost_build::Config::extern_path`. This makes imported object messages resolve
to the canonical generated types from this crate instead of generating duplicate Rust types in the
RPC crate.

## Decoded Objects

The experimental `ProtoDecodeFields` derive generates schema-shaped records, required/optional/
repeated message conversion, and nested field/index error paths. Atomic messages can implement
`DecodeMessage` using their existing representation decoder. Enum fields use Prost's named enum
types, preserving optional/repeated cardinality and rejecting unknown discriminants with generated
paths. Known variants such as `Unspecified` remain available for domain verification. Oneofs, maps,
and boxed messages are not yet supported by the derive.

Domain construction is opt-in and handwritten: `Verify::verify()` needs no external context,
`VerifyWith<C>::verify_with(context)` accepts borrowed or owned context, and
`BuildUnchecked::build_unchecked()` uses a supported unchecked constructor. Unchecked construction
can still fail and must document the invariants the caller must ensure. None of these operations
is invoked automatically by field decoding, and verification errors do not get generated wire
paths. Existing conversion APIs are retained as compatibility bridges during migration.

The current coverage and complete list of skipped messages are in
[Decoded Conversion Migration](DECODED_MIGRATION.md).

## License

This project is [MIT licensed](../../LICENSE).
