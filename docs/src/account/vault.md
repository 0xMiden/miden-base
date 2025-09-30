# Vault

> [!Note]
> A cryptographically committed container holding an account’s [assets](../asset.md).

The vault stores both fungible and non‑fungible assets and reduces to a single 32‑byte commitment (its Sparse Merkle Tree root). Only an account’s own code can modify its vault; external callers must go through the account’s exported procedures.

## Data structure

- **Sparse Merkle Tree (SMT)**: All assets are stored as leaves in an SMT. The root of this tree is the vault commitment. The vault depth equals the global `SMT_DEPTH` (exposed as `AssetVault::DEPTH`).
- **Keying rules**
  - **Fungible assets**: The leaf index is derived from the issuing faucet `AccountId`. There is at most one leaf per faucet; amounts aggregate when more of the same asset is added.
  - **Non‑fungible assets**: The leaf index is derived from the asset itself; each unique NF asset occupies its own leaf.

These rules allow the vault to contain an unbounded number of assets while keeping proofs logarithmic in the (sparse) key space.

## Operations

- **Add asset** (`account_add_asset`): Adds a fungible or non‑fungible asset to the vault. For fungible assets from the same faucet, amounts are summed.
  - Fails if the asset is invalid, the resulting fungible total would be ≥ 2^63, or a duplicate non‑fungible asset is added.
- **Remove asset** (`account_remove_asset`): Removes the specified asset from the vault.
  - Fails if the fungible asset is not found or its remaining amount would be negative, or if the non‑fungible asset is not found.

Implementations emit before/after events around vault mutations and maintain a per‑transaction `AccountVaultDelta` recording changes. The vault root updates deterministically with each modification.

## Commitment and capacity

- **Commitment**: The vault root is part of the account’s overall state commitment and can be retrieved via: `get_vault_root` (current account), `get_initial_vault_root` (at transaction start).
- **Capacity**: Conceptually unbounded; the SMT enables storing very large sets of assets with succinct inclusion/exclusion proofs.

## Related APIs

- Account procedures to access and mutate the vault are exposed via the Miden protocol library: `get_vault_root`, `get_initial_vault_root`, `account_add_asset`, `account_remove_asset`. These procedures enforce correct calling contexts so only the account itself can change its vault.

## See also

- [Assets](../asset.md) — fungible vs non‑fungible assets
- [Account Code](./code.md) — procedures that are allowed to modify storage and the vault
- [Miden Protocol Library](../protocol_library.md) — account procedures for working with the vault


