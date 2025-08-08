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

## Procedure tracking

> [!Warning]
> Procedure tracking is edge-triggered via kernel account APIs. Merely executing an account procedure does not guarantee it will be marked as "called".

Miden tracks whether an account procedure "was called" during a transaction using kernel support. A procedure is marked as called only when it invokes an authenticateable kernel account API that asserts the account context. Concretely, tracking is engaged when the call stack passes through:

- `tx::authenticate_account_origin` (which internally executes `account::authenticate_and_track_procedure`), or
- `tx::assert_auth_procedure_origin` (which internally executes `account::assert_auth_procedure`).

If a procedure executes only local MASM instructions and never calls into account-restricted kernel APIs (e.g., it just does `push.0 drop`), it will not be tracked, even though it was executed.

- Example: An authentication procedure like `auth__basic` verifies a signature and uses account APIs (e.g., `account::incr_nonce`), so it will be tracked and visible to `was_procedure_called`-based logic.
- Counter-example: A noop variant like `auth__noop` which only contains local instructions (e.g., `push.0 drop`) will not trigger tracking, thus `was_procedure_called` will remain false even if the procedure ran.

Implications:
- **ACLs keyed off tracked procedures** must ensure that the relevant procedures interact with account-restricted kernel APIs so that tracking is triggered.
- Current tracking records a boolean "was_called" per procedure. Counting the number of invocations is not yet recorded.

Rationale and future direction:
- Today’s approach is lightweight and implemented at the kernel level via `account::authenticate_and_track_procedure` and `account::assert_auth_procedure`. A future enhancement may introduce VM-level native tracking to unambiguously track account procedure calls and to enable per-procedure invocation counts.
