# Account Code

> [!Note]
> A collection of functions defining the `Account`'s programmable interface.

Every Miden `Account` is essentially a smart contract. The `Code` defines the account's functions, which can be invoked through both [note scripts](../note.md#script) and [transaction scripts](../transaction.md#inputs). Key characteristics include:

- **Mutable access:** Only the `Account`'s own functions can modify its storage and vault. All state changes — such as updating storage slots or transferring assets — must occur through these functions.
- **Function commitment:** Each function can be called by its [MAST](https://0xMiden.github.io/miden-vm/user_docs/assembly/main.html) root. The root represents the underlying code tree as a 32-byte commitment. This ensures integrity which means a function's behavior cannot change without changing the MAST root.
- **Asset creation:** Faucet `Accounts` can create assets.

An account's code is typically the result of merging multiple [account components](./component.md).

## Authentication

Authenticating a transaction, and therefore the changes to the account, is done with an _authentication procedure_. Every account's code must provide exactly one authentication procedure. It is automatically called during the transaction epilogue, i.e. after all note scripts and the transaction script have been executed.

Such an authentication procedure typically inspects the transaction and then decides whether a signature is required to authenticate the changes. It does this by:
- checking which account procedures have been called
  - Example: Authentication is required if the `distribute` procedure was called but not if `burn` was called.
- inspecting the account delta.
  - Example: Authentication is required if a cryptographic key in storage was updated.
  - Example: Authentication is required if an asset was removed from the vault.
- checking whether notes have been consumed.
- checking whether notes have been created.

Recall that an [account's nonce](overview.md#nonce) must be incremented whenever its state changes. Only authentication procedures are allowed to do so, to prevent accidental or unintended authorization of state changes.

### Procedure tracking
The authentication procedure can base its decision on whether a specific account procedure was called during the transaction (procedure tracking). A procedure is considered "tracked" only if it invokes account-restricted kernel APIs (e.g., via `exec.faucet::mint`). Procedures that execute only local instructions (e.g., a noop `push.0 drop`) will not be marked as tracked.
