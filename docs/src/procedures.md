# Procedures and Invocation Contexts

> [!Note]
> This page clarifies which library procedures can be called from account code vs. from scripts (note/transaction), and why this distinction exists.
>
> Context: see discussion in “Differentiate between library exports usable in scripts vs. in account procedures” [issue #1554](https://github.com/0xMiden/miden-base/issues/1554).

Miden programs run in three main contexts:

- Account procedures: methods that make up an account’s interface and internal implementation. See `Accounts → Code`.
- Note scripts: logic attached to notes, executed during note processing.
- Transaction scripts: executor-provided logic executed after notes. See `Transactions`.

Each context has different permissions. As a rule of thumb:

- Scripts (note/transaction) can read from the environment and accounts, but cannot mutate account state directly.
- Account procedures can both read and mutate the account’s storage and vault, and are the only place where the account nonce may be incremented (via the authentication procedure).

See also:
- `Accounts → Code` for authentication and account method semantics: ../account/code.md
- `Transactions` for lifecycle and examples: ../transaction.md

## What can be called from where?

Below are common categories with typical callability. Names are illustrative and not exhaustive; always prefer calling account-exposed methods for state changes.

- Read-only account queries (e.g., `account::get_id`, reading transaction context):
  - Account procedures: allowed
  - Note/Transaction scripts: allowed

- Reading note inputs and transaction args (e.g., `note::get_inputs`):
  - Account procedures: allowed
  - Note/Transaction scripts: allowed

- Mutating account storage (e.g., key/value writes such as `account::set_item`):
  - Account procedures: allowed
  - Note/Transaction scripts: disallowed directly. Must call an account method that performs the write.

- Mutating the account vault (e.g., `account::add_asset`, `account::remove_asset`):
  - Account procedures: allowed
  - Note/Transaction scripts: disallowed directly. Must go through an exposed account method (e.g., a wallet component’s receive/move functions).

- Authentication / nonce increment:
  - Only the account’s authentication procedure may increment nonce. Scripts cannot do this directly. See ../account/code.md#authentication

- Creating notes:
  - Typically done via account methods (e.g., wallet component methods) that scripts can invoke. Scripts should not bypass the account interface.

## Why this separation?

Encapsulation and safety. Accounts are smart contracts with explicit interfaces. Scripts (note and transaction) execute against an account but must not bypass its interface to mutate state. This guarantees that all state changes respect the account’s access control, authorization, and invariants.

In practice, scripts should:
- Prefer calling account-exposed methods for any operation that changes storage or vault state.
- Use read-only/environment helpers directly when needed.

Account procedures should:
- Encapsulate state changes (storage writes, vault changes, note creation) behind clearly named methods.
- Drive authentication by inspecting which account methods were invoked and what changed. See ../account/code.md#authentication

## Examples (non-exhaustive)

- Valid in scripts:
  - `account::get_id` to check the target account ID in a note script.
  - `note::get_inputs` to parse recipient parameters.
  - Calling `wallets::basic::create_note` and `wallets::basic::move_asset_to_note` via the account interface from a transaction script. See examples in ../transaction.md.

- Invalid directly in scripts (must go via account methods):
  - `account::set_item` (storage write)
  - `account::add_asset` / `account::remove_asset` (vault mutation)

## Naming guidance and future directions

As tracked in [issue #1554](https://github.com/0xMiden/miden-base/issues/1554), some exports under `miden::account` are safe for scripts (read-only), while others are account-only (mutating). To improve clarity, future versions may further differentiate namespaces between general-purpose procedures callable from scripts and account-only procedures. Until then, follow the rules above and treat any mutating `account::...` routines as account-only unless explicitly documented otherwise.