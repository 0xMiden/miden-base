# Account Code

The `Code` component of an account defines its programmable interface and logic. In Miden, every account is essentially a smart contract, and its code determines how it can interact with assets, storage, and the blockchain state.

## Structure of Account Code

Account code is organized as a collection of functions. These functions can:
- Modify the account's storage and vault
- Increment the nonce
- Transfer assets
- Create new notes

Each function is committed to via a [MAST](https://0xMiden.github.io/miden-vm/user_docs/assembly/main.html) root, ensuring code integrity and verifiability.

## Account Components

Account code can be modularized using **account components**. Each component encapsulates a specific piece of functionality and its associated storage layout. Components can be combined to form the complete code of an account.

### Component Templates

A **component template** provides a reusable blueprint for account components. It defines:
- **Metadata:** Name, description, version, and supported account types
- **Code:** A library of functions operating on a defined storage layout
- **Storage Layout:** A contiguous list of storage slots, with optional initial values or placeholders

#### Example: Component Template in TOML

```toml
name = "Fungible Faucet"
description = "This component showcases the component template format."
version = "1.0.0"
supported-types = ["FungibleFaucet"]

[[storage]]
name = "token_metadata"
description = "Metadata about the token."
slot = 0
value = [
    { type = "felt", name = "max_supply" },
    { type = "token_symbol", value = "TST" },
    { type = "u8", name = "decimals", value = "10" },
    { value = "0x0" }
]
```

#### Storage Entries in Templates
- **Single-slot value:** Fits in one slot (word)
- **Multi-slot value:** Spans multiple contiguous slots
- **Storage map:** Key-value pairs, both as words

##### Single-slot value

A single-slot value fits within one slot (i.e., one word).

For a single-slot entry, the following fields are expected:

- `slot`: Specifies the slot index in which the value will be placed
- `value` (optional): Contains the initial storage value for this slot. Will be interpreted as a `word` unless another `type` is specified
- `type` (optional): Describes the expected type for the slot

If no `value` is provided, the entry acts as a placeholder, requiring a value to be passed at instantiation. In this case, specifying a `type` is mandatory to ensure the input is correctly parsed. So the rule is that at least one of `value` and `type` has to be specified.

Valid types for a single-slot value are `word` or `auth::rpo_falcon512::pub_key`.

In the example above, the first and second storage entries are single-slot values.

##### Storage map entries

A storage map consists of key-value pairs, where both keys and values are single words.

Storage map entries can specify the following fields:

- `slot`: Specifies the slot index in which the root of the map will be placed
- `values`: Contains a list of map entries, defined by a `key` and `value`

Where keys and values are word values, which can be defined as placeholders.

Example:

```toml
[[storage]]
name = "map_storage_entry"
slot = 2
values = [
    { key = "0x1", value = ["0x0", "249381274", "998123581", "124991023478"] },
    { key = "0xDE0B1140012A9FD912F18AD9EC85E40F4CB697AE", value = { name = "value_placeholder" } }
]
```

In the example, the third storage entry defines a storage map.

#### Value Types
- `word` (default), `auth::rpo_falcon512::pub_key` (for Falcon public keys)
- Field elements: `u8`, `u16`, `u32`, `felt`, `token_symbol`

#### Placeholders
Placeholders can be used in templates to require values at instantiation time, ensuring flexibility and reusability.

---

For more details on storage, see [Storage](./storage.md). 
