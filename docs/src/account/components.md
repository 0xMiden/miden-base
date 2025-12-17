---
sidebar_position: 6
title: "Components"
---

# Account Components

Account components are reusable units of functionality that define a part of an account's code and storage. Multiple account components can be merged together to form an account's final [code](./code) and [storage](./storage).

As an example, consider a typical wallet account, capable of holding a user's assets and requiring authentication whenever assets are added or removed. Such an account can be created by merging a `BasicWallet` component with an `RpoFalcon512` authentication component. The basic wallet does not need any storage, but contains the code to move assets in and out of the account vault. The authentication component holds a user's public key in storage and additionally contains the code to verify a signature against that public key. Together, these components form a fully functional wallet account.

## Account Component schemas

An account component schema describes a reusable piece of account functionality and captures
everything required to initialize it. The schema encapsulates the component's **metadata**, its
code, and how its storage should be laid out and typed.

Once defined, a component schema can be instantiated to an account component, which can then be
merged to form the account's `Code` and `Storage`.

## Component code

The component's code defines a library of functions that can read and write to the storage slots specified by the schema.

## Component metadata

The component metadata describes the account component entirely: its name, description, version, and storage layout.

The storage layout is described as a set of named storage slots. Each slot name must be a valid
[`StorageSlotName`](../api/miden_objects/account/struct.StorageSlotName.html), and its slot ID is
derived deterministically from the name.

A slot can either define a concrete value (optionally containing typed fields filled at instantiation),
or declare a whole-slot type whose value is supplied at instantiation time.

### TOML specification

The component metadata can be defined using TOML. Below is an example specification:

```toml
name = "Fungible Faucet"
description = "This component showcases the component schema format, and the different ways of providing valid values to it."
version = "1.0.0"
supported-types = ["FungibleFaucet"]

[[storage]]
name = "demo::token_metadata"
description = "Contains token metadata (max supply, symbol, decimals)."
type = [
    { type = "u32", name = "max_supply", description = "Maximum supply of the token in base units" },
    { type = "token_symbol", name = "symbol", description = "Token symbol", default-value = "TST" },
    { type = "u8", name = "decimals", description = "Number of decimal places for converting to absolute units" },
    { type = "void" }
]

[[storage]]
name = "demo::owner_public_key"
description = "This is a typed value supplied at instantiation and interpreted as a Falcon public key"
type = "auth::rpo_falcon512::pub_key"

[[storage]]
name = "demo::protocol_version"
description = "A whole-word init-supplied value typed as a felt (stored as [0,0,0,<value>])."
type = "u8"

[[storage]]
name = "demo::static_map"
description = "A map slot with statically defined entries"
type = "map"
default-values = [
    { key = "0x0000000000000000000000000000000000000000000000000000000000000001", value = ["0x0", "249381274", "998123581", "124991023478"] },
    { key = ["0", "0", "0", "2"], value = "0x0000000000000000000000000000000000000000000000000000000000000010" }
]

[[storage]]
name = "demo::procedure_thresholds"
description = "Map which stores procedure thresholds (PROC_ROOT -> signature threshold)"
type = "map"
key-type = "word"
value-type = "u16"
```

#### Specifying word schema and types

Value-slot entries describe their schema via `WordSchema`. A value type can be either:

- **Singular**: defined through the `type` field, indicating the expected `SchemaTypeIdentifier` for the entire word. The value is supplied at instantiation time via `InitStorageData`.
- **Composed**: provided through `type = [ ... ]`, which contains exactly four `FeltSchema` descriptors. Each element is either a named typed field (optionally with `default-value`) or a `void` element for reserved/padding zeros.

Composed schema entries reuse the existing TOML structure for four-element words, while singular schemas rely on `type`. In our example, the `token_metadata` slot uses a composed schema (`type = [...]`) mixing typed fields (`max_supply`, `decimals`) with defaults (`symbol`) and a reserved/padding `void` element.

##### Word schema example

```toml
[[storage]]
name = "demo::faucet_id"
description = "Account ID of the registered faucet"
type = [
  { type = "felt", name = "prefix", description = "Faucet ID prefix" },
  { type = "felt", name = "suffix", description = "Faucet ID suffix" },
  { type = "void" },
  { type = "void" },
]
```

##### Word types

Singular schemas accept `word` (default) and word-shaped types such as `auth::rpo_falcon512::pub_key` or `auth::ecdsa_k256_keccak::pub_key` (parsed from hexadecimal strings).

Singular schemas can also use any felt type (e.g. `u8`, `u16`, `u32`, `felt`, `token_symbol`, `void`). The value is parsed as a felt and stored as a word with the parsed felt in the last element and the remaining elements set to `0`.

##### Felt types

Valid field element types are `void`, `u8`, `u16`, `u32`, `felt` (default) and `token_symbol`:

- `void` is a special type which always evaluates to `0` and does not produce an init requirement; it is intended for reserved or padding elements.
- `u8`, `u16` and `u32` values can be parsed as decimal numbers and represent 8-bit, 16-bit and 32-bit unsigned integers.
- `felt` values represent a field element, and can be parsed as decimal or hexadecimal numbers.
- `token_symbol` values represent basic fungible token symbols, parsed as 1–6 uppercase ASCII characters.

#### Header

The metadata header specifies four fields:

- `name`: The component schema's name
- `description` (optional): A brief description of the component schema and its functionality
- `version`: A semantic version of this component schema
- `supported-types`: Specifies the types of accounts on which the component can be used. Valid values are `FungibleFaucet`, `NonFungibleFaucet`, `RegularAccountUpdatableCode` and `RegularAccountImmutableCode`

#### Storage entries

An account component schema can contain multiple storage entries, each describing either a
**single-slot value** or a **storage map**. Every entry carries:

- `name`: Identifies the storage entry.
- `description` (optional): Explains the entry's purpose within the component.

The remaining fields depend on whether the entry is a value slot or a map slot.

##### Single-slot value

Single-slot entries are represented by `ValueSlotSchema` and occupy one slot (one word). They use the fields:

- `type` (optional): Describes the schema for this slot. It can be either:
  - a string type identifier (singular init-supplied slot), or
  - an array of 4 felt schema descriptors (composed slot schema).
- `default-value` (optional): An overridable default for singular slots. If omitted, the slot is required at instantiation (unless `type = "void"`).

In our TOML example, the first entry defines a composed schema, while the second is an init-supplied value typed as `auth::rpo_falcon512::pub_key`.

##### Storage map entries

[Storage maps](./storage#map-slots) use `MapSlotSchema` and describe key-value pairs where each key and value is itself a `WordSchema`. Map slots support:

- `type = "map"`: Declares that this entry is a map slot.
- `key-type` (optional): Declares the schema/type of keys stored in the map.
- `value-type` (optional): Declares the schema/type of values stored in the map.
- `default-values` (optional): Lists default map entries defined by nested `key` and `value` descriptors. Each entry must be fully specified and cannot contain typed fields.

`key-type` / `value-type` accept either a string type identifier (e.g. `"word"`) or a 4-element array of `FeltSchema` descriptors.

If `default-values` is omitted, the map is populated at instantiation via [`InitStorageData`](#providing-init-values). When `default-values` are present, they act as defaults: init data can optionally add entries and override existing keys.

In the example, the third storage entry defines a static map and the fourth entry (`procedure_thresholds`) is populated at instantiation.

##### Typed map example

You can type maps at the slot level via `key-type` and `value-type` (each a `WordSchema`):

```toml
[[storage]]
name = "demo::typed_map"
type = "map"
key-type = "word"
value-type = "auth::rpo_falcon512::pub_key"
```

This declares that all keys are `word` and all values are `auth::rpo_falcon512::pub_key`, regardless of whether the map contents come from `default-values = [...]` (static) or are supplied at instantiation via `InitStorageData`.

`key-type` / `value-type` are validated when building map entries from `InitStorageData` (and when validating `default-values`).

##### Multi-slot value

Multi-slot values are currently unsupported by component schemas.

#### Providing init values

When a storage entry requires init-supplied values, an implementation must provide their concrete values
at instantiation time. This is done through `InitStorageData` (available as `miden_objects::account::component::InitStorageData`), which can be created programmatically or loaded from TOML using `InitStorageData::from_toml()`.

For example, the init-populated map entry above can be populated from TOML as follows:

```toml
"demo::owner_public_key" = "0x1234"
"demo::protocol_version" = 1

["demo::token_metadata"]
max_supply = 1000000000
decimals = 10

"demo::procedure_thresholds" = [
    {
      key = "0xd2d1b6229d7cfb9f2ada31c5cb61453cf464f91828e124437c708eec55b9cd07",
      value = ["0", "0", "0", "1"]
    },
    {
      key = "0x2217cd9963f742fc2d131d86df08f8a2766ed17b73f1519b8d3143ad1c71d32d",
      value = ["0", "0", "0", "2"]
    }
]
```

Each element in the array is a fully specified key/value pair. Keys and values can be written either as hexadecimal words or as an array of four field elements (decimal or hexadecimal strings). Note that slot names include `::`, so they must be quoted in TOML. This syntax complements the existing `default-values = [...]` form used for static maps, and mirrors how map entries are provided in component metadata. If an init-populated map slot is omitted from `InitStorageData`, it defaults to an empty map.
