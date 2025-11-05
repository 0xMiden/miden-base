---
sidebar_position: 6
title: "Components"
---

# Account Components

Account components are reusable units of functionality that define a part of an account's code and storage. Multiple account components can be merged together to form an account's final [code](./code) and [storage](./storage).

As an example, consider a typical wallet account, capable of holding a user's assets and requiring authentication whenever assets are added or removed. Such an account can be created by merging a `BasicWallet` component with an `RpoFalcon512` authentication component. The basic wallet does not need any storage, but contains the code to move assets in and out of the account vault. The authentication component holds a user's public key in storage and additionally contains the code to verify a signature against that public key. Together, these components form a fully functional wallet account.

## Account Component templates

An account component template is a description of an account component and contains all the
information needed to initialize it.

Specifically, a template specifies a component's **metadata** and its **code**.

Once defined, a component template can be instantiated to an account component, which can then be
merged to form the account's `Code` and `Storage`.

## Component code

The template's code defines a library of functions that can read and write to the storage slots of the component.

## Component metadata

The component metadata describes the account component entirely: its name, description, version, and storage layout.

The storage layout must specify a contiguous list of slot values that starts at index `0`, and can optionally specify initial values for each of the slots. Alternatively, placeholders can be utilized to identify values that should be provided at the moment of instantiation.

### TOML specification

The component metadata can be defined using TOML. Below is an example specification:

```toml
name = "Fungible Faucet"
description = "This component showcases the component template format, and the different ways of
providing valid values to it."
version = "1.0.0"
supported-types = ["FungibleFaucet"]

[[storage]]
name = "token_metadata"
description = "Contains metadata about the token associated to the faucet account. The metadata
is formed by three fields: max supply, the token symbol and the asset's decimals"
slot = 0
value = [
    { type = "felt", name = "max_supply", description = "Maximum supply of the token in base units" },
    { type = "token_symbol", value = "TST" },
    { type = "u8", name = "decimals", description = "Number of decimal places for converting to absolute units", value = "10" },
    { value = "0x0" }
]

[[storage]]
name = "owner_public_key"
description = "This is a value placeholder that will be interpreted as a Falcon public key"
slot = 1
type = "auth::rpo_falcon512::pub_key"

[[storage]]
name = "map_storage_entry"
slot = 2
values = [
    { key = "0x1", value = ["0x0", "249381274", "998123581", "124991023478"] },
    {
      key = "0xDE0B1140012A9FD912F18AD9EC85E40F4CB697AE",
      value = {
        name = "value_placeholder",
        description = "This value will be defined at the moment of instantiation"
      }
    }
]

[[storage]]
name = "procedure_thresholds"
description = "Map which stores procedure thresholds (PROC_ROOT -> signature threshold)"
slot = 3
type = "map"

[[storage]]
name = "multislot_entry"
slots = [4,5]
values = [
    ["0x1","0x2","0x3","0x4"],
    ["50000","60000","70000","80000"]
]
```

#### Specifying values and their types

In the TOML format, any value that is one word long can be written as a single value, or as exactly four field elements. In turn, a field element is a number within Miden's finite field.

A word can be written as a hexadecimal value, and field elements can be written either as hexadecimal or decimal numbers. In all cases, numbers should be input as strings.

In our example, the `token_metadata` single-slot entry is defined as four elements, where the first element is a placeholder, and the second, third and fourth are hardcoded values.

##### Word types

Valid word types are `word` (default type) and `auth::rpo_falcon512::pub_key` (represents a Falcon public key). Both can be written and interpreted as hexadecimal strings.

##### Felt types

Valid field element types are `u8`, `u16`, `u32`, `felt` (default type) and `token_symbol`:

- `u8`, `u16` and `u32` values can be parsed as decimal numbers and represent 8-bit, 16-bit and 32-bit unsigned integers
- `felt` values represent a field element, and can be parsed as decimal or hexadecimal values
- `token_symbol` values represent the symbol for basic fungible tokens, and are parsed as strings made of four uppercase characters

#### Header

The metadata header specifies four fields:

- `name`: The component template's name
- `description` (optional): A brief description of the component template and its functionality
- `version`: A semantic version of this component template
- `supported-types`: Specifies the types of accounts on which the component can be used. Valid values are `FungibleFaucet`, `NonFungibleFaucet`, `RegularAccountUpdatableCode` and `RegularAccountImmutableCode`

#### Storage entries

An account component template can have multiple storage entries. A storage entry can specify either a **single-slot value**, a **multi-slot value**, or a **storage map**.

Each of these storage entries contain the following fields:

- `name`: A name for identifying the storage entry
- `description` (optional): Describes the intended function of the storage slot within the component definition

Additionally, based on the type of the storage entry, there are specific fields that should be specified.

##### Single-slot value

A single-slot value fits within one slot (i.e., one word).

For a single-slot entry, the following fields are expected:

- `slot`: Specifies the slot index in which the value will be placed
- `value` (optional): Contains the initial storage value for this slot. Will be interpreted as a `word` unless another `type` is specified
- `type` (optional): Describes the expected type for the slot

If no `value` is provided, the entry acts as a placeholder, requiring a value to be passed at instantiation. In this case, specifying a `type` is mandatory to ensure the input is correctly parsed. So the rule is that at least one of `value` and `type` has to be specified.
Valid types for a single-slot value are `word` or `auth::rpo_falcon512::pub_key`.

In the above example, the first and second storage entries are single-slot values.

##### Storage map entries

[Storage maps](./storage#map-slots) consist of key-value pairs, where both keys and values are single words.

Storage map entries can specify the following fields:

- `slot`: Specifies the slot index in which the root of the map will be placed
- `values` (optional): Contains a list of map entries, defined by a `key` and `value`. Each entry is
  interpreted as a word, and keys or values may themselves be expressed via placeholders.
- `type = "map"` (optional): When provided without `values`, the entry is treated as a templated map
  whose contents must be provided at instantiation time through [`InitStorageData`](#initializing-placeholder-values).
  If `values` are present, the entry is interpreted as a static map regardless of the `type` field, so
  specifying `type = "map"` becomes purely descriptive in that case.

In the example, the third storage entry defines a static storage map with two initial entries, while
the fourth entry (`procedure_thresholds`) is a templated map whose contents are supplied at
instantiation time.

##### Multi-slot value

Multi-slot values are composite values that exceed the size of a single slot (i.e., more than one `word`).

For multi-slot values, the following fields are expected:

- `slots`: Specifies the list of contiguous slots that the value comprises
- `values`: Contains the initial storage value for the specified slots

Placeholders can currently not be defined for multi-slot values. In our example, the fifth entry defines a two-slot value.

#### Initializing placeholder values

When a storage entry introduces placeholders, an implementation must provide their concrete values
at instantiation time. This is done through `InitStorageData` (available as `miden_objects::account::InitStorageData`), which can be created programmatically or loaded from TOML using `InitStorageData::from_toml()`.

For example, the templated map entry above can be populated from TOML as follows:

```toml
procedure_thresholds = [
    {
      key = "0xd2d1b6229d7cfb9f2ada31c5cb61453cf464f91828e124437c708eec55b9cd07",
      value = "0x00000000000000000000000000000000000000000000000000000000000001"
    },
    {
      key = "0x2217cd9963f742fc2d131d86df08f8a2766ed17b73f1519b8d3143ad1c71d32d",
      value = ["0", "0", "0", "2"]
    }
]
```

Each element in the array is a fully specified key/value pair. Keys and values can be written either as hexadecimal words or as an array of four field elements (decimal or hexadecimal strings). This syntax complements the existing `values = [...]` form used for static maps, and mirrors how map entries are provided in component metadata.
