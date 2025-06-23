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

#### Value Types
- `word` (default), `auth::rpo_falcon512::pub_key` (for Falcon public keys)
- Field elements: `u8`, `u16`, `u32`, `felt`, `token_symbol`

#### Placeholders
Placeholders can be used in templates to require values at instantiation time, ensuring flexibility and reusability.

---

For more details on storage, see [Storage](./storage.md). 
