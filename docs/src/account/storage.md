# Account Storage

> [!Note]
> A flexible, arbitrary data store within the `Account`.

The [storage](https://docs.rs/miden-objects/latest/miden_objects/account/struct.AccountStorage.html) is divided into a maximum of 255 indexed [storage slots](https://docs.rs/miden-objects/latest/miden_objects/account/enum.StorageSlot.html). Each slot can either store a 32-byte value or serve as the cryptographic root to a key-value store with the capacity to store large amounts of data.

- **Value slots:** Contains 32 bytes of arbitrary data.
- **Map slots:** Contains a [StorageMap](#storagemap), a key-value store where both keys and values are 32 bytes. The slot's value is a commitment to the entire map.

An account's storage is typically the result of merging multiple [account components](./component.md).

## Value Slots

A value slot can be used whenever 32 bytes of data is enough, e.g. for storing a single public key for use in [authentication procedures](code.md#authentication).

## Map Slots

A map slot contains a `StorageMap` which is a key-value store implemented as a sparse Merkle tree (SMT) of depth 64. This allows an account to store a much larger amount of data than would be possible using only the account's storage slots. The root of the underlying SMT is stored in a single account storage slot, and each map entry is a leaf in the tree. When retrieving an entry (e.g., via `account::get_map_item`), its inclusion is proven using a Merkle proof.

Key properties of `StorageMap`:

- **Efficient, scalable storage:** The SMT structure enables efficient storage and proof of inclusion for a large number of entries, while only storing the root in the account's storage slot.
- **Partial presence:** Not all entries of the map need to be present at transaction execution time to access or modify the map. It is sufficient if only the accessed or modified items are present in the advice provider.
- **Key hashing:** Since map keys are user-chosen and may not be uniformly distributed, keys are hashed before being inserted into the SMT. This ensures a more balanced tree and mitigates efficiency issues due to key clustering. The original keys are retained in a separate map, allowing for introspection (e.g., querying the set of stored original keys for debugging or explorer scenarios). This introduces some redundancy, but enables useful features such as listing all stored keys.

This design allows for flexible, scalable, and privacy-preserving storage within accounts, supporting both large datasets and efficient proof generation.
