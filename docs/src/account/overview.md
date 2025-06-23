# Accounts Overview

An `Account` is the primary entity in the Miden protocol, capable of holding assets, storing data, and executing custom code. Accounts serve as specialized smart contracts, providing a programmable interface for interacting with their state and assets.

## Purpose of Accounts

In Miden's hybrid UTXO- and account-based model, accounts enable expressive smart contracts via a Turing-complete language. They allow users and developers to define custom logic, manage assets, and interact with the blockchain state securely and flexibly.

## Account Lifecycle

Accounts progress through several phases:

- **Creation and Deployment:** Initialization of the account on the network.
- **Active Operation:** State updates via account functions that modify storage, nonce, and vault.
- **Termination or Deactivation:** Optional, depending on contract design and governance.

### Account Creation Process

1. A user generates a new account ID locally using the Miden client.
2. The client checks with a Miden node to ensure the ID is unique.
3. The user can share the new ID to receive assets or interact with others.
4. The account becomes recognized network-wide once included in the account database.

## Account Types

Miden supports several account types:

- **Basic Accounts:**
  - *Mutable:* Code can be changed after deployment.
  - *Immutable:* Code cannot be changed once deployed.
- **Faucets:**
  - Always immutable. Can issue fungible or non-fungible assets.

Type and mutability are encoded in the most significant bits of the account's ID.

## Account Storage Modes

- **Public Accounts:** State is stored on-chain and accessible to all.
- **Network Accounts:** Public accounts monitored by the network for incoming notes and transactions.
- **Private Accounts:** Only a commitment to the state is stored on-chain, suitable for privacy or large data.

The storage mode is chosen at creation and cannot be changed later.

---

Continue to [Code](./code.md) or [Storage](./storage.md) for more details. 
