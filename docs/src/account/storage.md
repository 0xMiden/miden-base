# Account Storage

Account storage in Miden is designed to be flexible, scalable, and privacy-preserving. It allows accounts to store arbitrary data, manage large datasets, and provide efficient proofs of inclusion.

## Storage Structure

Account storage is divided into up to 255 indexed **storage slots**. Each slot can store either:
- A 32-byte value (`StorageSlot::Value`)
- A pointer to a key-value store (`StorageSlot::Map`)

### Storage Slots

Each storage slot can hold a single 32-byte value or a commitment to a storage map. Slots are indexed from 0 to 254.

### Storage Maps

A **StorageMap** is a key-value store implemented as a sparse Merkle tree (SMT) of depth 64. Both keys and values are 32 bytes. The root of the SMT is stored in a single storage slot, and each map entry is a leaf in the tree.

#### Key Properties
- **Efficient, scalable storage:** SMT enables efficient storage and proof of inclusion for large datasets.
- **Partial presence:** Only accessed or modified items need to be present during transaction execution.
- **Key hashing:** Keys are hashed before insertion to ensure a balanced tree and efficient lookups.

#### Example: Storage Map Entry in a Component Template

```toml
[[storage]]
name = "map_storage_entry"
slot = 2
values = [
    { key = "0x1", value = ["0x0", "249381274", "998123581", "124991023478"] },
    { key = "0xDE0B1140012A9FD912F18AD9EC85E40F4CB697AE", value = { name = "value_placeholder" } }
]
```

### Multi-slot Values

Some data may span multiple contiguous slots. Multi-slot values are defined by specifying a list of slots and their initial values.

#### Example: Multi-slot Entry

```toml
[[storage]]
name = "multislot_entry"
slots = [3,4]
values = [
    ["0x1","0x2","0x3","0x4"],
    ["50000","60000","70000","80000"]
]
```

## Storage Modes

- **Public:** State is stored on-chain and accessible to all.
- **Private:** Only a commitment to the state is stored on-chain.

## Illustration

![Account Storage Structure](../img/account/account-definition.png)

---

For more on account code and components, see [Code](./code.md). 
