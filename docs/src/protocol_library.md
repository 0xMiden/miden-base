---
sidebar_position: 8
---

# Miden Protocol Library

The Miden protocol library provides a set of procedures that wrap transaction kernel procedures to provide a more convenient interface for common operations. These can be invoked by account code, note scripts, and transaction scripts, though some have restriction from where they can be called. The procedures are organized into modules corresponding to different functional areas.

## Contexts

The Miden VM contexts from which procedures can be called are:

- **Account**: Can only be called from native or foreign accounts.
  - **Native**: Can only be called when the current account is the native account.
  - **Auth**: Can only be called from the authentication procedure. Since it is called on the native account, it implies **Native** and **Account**.
  - **Faucet**: Can only be called when the current account is a faucet.
- **Note**: Can only be called from a note script.
- **Any**: Can be called from any context.

If a procedure has multiple context requirements they are combined using `&`. For instance, "Native & Account" means the procedure can only be called when the current account is the native one _and_ only from the account context.

## Implementation

Most procedures in the Miden protocol library are implemented as wrappers around underlying kernel procedures. They handle the necessary stack padding and cleanup operations required by the kernel interface, providing a more convenient API for developers.

The procedures maintain the same security and context restrictions as the underlying kernel procedures. When invoking these procedures, ensure that the calling context matches the requirements.

## Account Procedures (`miden::account`)

Account procedures can be used to read and write to account storage, add or remove assets from the vault and fetch or compute commitments.

| Procedure                        | Description                                                                                                                                                                                               | Context          |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| `get_id`                         | Returns the ID of the current account.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[account_id_prefix, account_id_suffix]`                                                                                | Any              |
| `get_native_id`                  | Returns the ID of the native account of the transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[account_id_prefix, account_id_suffix]`                                                              | Any              |
| `get_nonce`                      | Returns the nonce of the current account. Always returns the initial nonce as it can only be incremented in auth procedures.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[nonce]`                         | Any              |
| `get_native_nonce`               | Returns the nonce of the native account of the transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[nonce]`                                                                                          | Any              |
| `incr_nonce`                     | Increments the account nonce by one and returns the new nonce. Can only be called from auth procedures.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[final_nonce]`                                        | Auth             |
| `get_initial_commitment`         | Returns the native account commitment at the beginning of the transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[INIT_COMMITMENT]`                                                                 | Any              |
| `compute_current_commitment`     | Computes and returns the account commitment from account data stored in memory.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[ACCOUNT_COMMITMENT]`                                                         | Any              |
| `compute_delta_commitment`       | Computes the commitment to the native account's delta. Can only be called from auth procedures.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[DELTA_COMMITMENT]`                                           | Auth             |
| `get_item`                       | Gets an item from the account storage.<br/><br/>**Inputs:** `[index]`<br/>**Outputs:** `[VALUE]`                                                                                                          | Account          |
| `get_initial_item`               | Gets the initial item from the account storage slot as it was at the beginning of the transaction.<br/><br/>**Inputs:** `[index]`<br/>**Outputs:** `[VALUE]`                                              | Account          |
| `set_item`                       | Sets an item in the account storage.<br/><br/>**Inputs:** `[index, VALUE]`<br/>**Outputs:** `[OLD_VALUE]`                                                                                                 | Native & Account |
| `get_map_item`                   | Returns the VALUE located under the specified KEY within the map contained in the given account storage slot.<br/><br/>**Inputs:** `[index, KEY]`<br/>**Outputs:** `[VALUE]`                              | Account          |
| `get_initial_map_item`           | Gets the initial VALUE from the account storage map as it was at the beginning of the transaction.<br/><br/>**Inputs:** `[index, KEY]`<br/>**Outputs:** `[VALUE]`                                         | Account          |
| `set_map_item`                   | Sets VALUE under the specified KEY within the map contained in the given account storage slot.<br/><br/>**Inputs:** `[index, KEY, VALUE]`<br/>**Outputs:** `[OLD_MAP_ROOT, OLD_MAP_VALUE]`                | Native & Account |
| `get_code_commitment`            | Gets the account code commitment of the current account.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[CODE_COMMITMENT]`                                                                                   | Account          |
| `get_initial_storage_commitment` | Returns the storage commitment of the native account at the beginning of the transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[INIT_STORAGE_COMMITMENT]`                                          | Any              |
| `compute_storage_commitment`     | Computes the latest account storage commitment of the current account.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[STORAGE_COMMITMENT]`                                                                  | Account          |
| `get_balance`                    | Returns the balance of the fungible asset associated with the provided faucet_id in the current account's vault.<br/><br/>**Inputs:** `[faucet_id_prefix, faucet_id_suffix]`<br/>**Outputs:** `[balance]` | Any              |
| `get_initial_balance`            | Returns the balance of the fungible asset associated with the provided faucet_id in the current account's vault at the beginning of the transaction.<br/><br/>**Inputs:** `[faucet_id_prefix, faucet_id_suffix]`<br/>**Outputs:** `[init_balance]` | Any              |
| `has_non_fungible_asset`         | Returns a boolean indicating whether the non-fungible asset is present in the current account's vault.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[has_asset]`                                      | Any              |
| `add_asset`                      | Adds the specified asset to the vault. For fungible assets, returns the total after addition.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[ASSET']`                                                  | Native & Account |
| `remove_asset`                   | Removes the specified asset from the vault.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[ASSET]`                                                                                                     | Native & Account |
| `get_initial_vault_root`         | Returns the vault root of the native account at the beginning of the transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[INIT_VAULT_ROOT]`                                                          | Any              |
| `get_vault_root`                 | Returns the vault root of the current account.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[VAULT_ROOT]`                                                                                                  | Any              |
| `was_procedure_called`           | Returns 1 if a procedure was called during transaction execution, and 0 otherwise.<br/><br/>**Inputs:** `[PROC_ROOT]`<br/>**Outputs:** `[was_called]`                                                     | Any              |
| `get_num_procedures`             | Returns the number of procedures in the current account.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[num_procedures]`                                                                                     | Any              |
| `get_procedure_root`             | Returns the procedure root for the procedure at the specified index.<br/><br/>**Inputs:** `[index]`<br/>**Outputs:** `[PROC_ROOT]`                                                                         | Any              |

## Active Note Procedures (`miden::active_note`)

Active note procedures can be used to fetch data from the note that is currently being processed by the transaction kernel.

| Procedure               | Description                                                                                                                                                     | Context |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| `get_assets`            | Writes the [assets](note.md#assets) of the active note into memory starting at the specified address.<br/><br/>**Inputs:** `[dest_ptr]`<br/>**Outputs:** `[num_assets, dest_ptr]` | Note    |
| `get_recipient`         | Returns the [recipient](note.md#note-recipient-restricting-consumption) of the active note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[RECIPIENT]`                                                              | Note    |
| `get_inputs`            | Writes the note's [inputs](note.md#inputs) to the specified memory address.<br/><br/>**Inputs:** `[dest_ptr]`<br/>**Outputs:** `[num_inputs, dest_ptr]`                           | Note    |
| `get_metadata`          | Returns the [metadata](note.md#metadata) of the active note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[METADATA]`                                                              | Note    |
| `get_sender`            | Returns the sender of the active note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[sender_id_prefix, sender_id_suffix]`                                        | Note    |
| `get_serial_number`     | Returns the [serial number](note.md#serial-number) of the active note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[SERIAL_NUMBER]`                                                      | Note    |
| `get_script_root`       | Returns the [script root](note.md#script) of the active note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[SCRIPT_ROOT]`                                                          | Note    |
| `add_assets_to_account` | Adds all assets from the active note to the account vault.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[]`                                                      | Note    |

## Input Note Procedures (`miden::input_note`)

Input note procedures can be used to fetch data on input notes consumed by the transaction.

| Procedure           | Description                                                                                                                                                                                                                    | Context |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------- |
| `get_assets_info`   | Returns the information about [assets](note.md#assets) in the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[ASSETS_COMMITMENT, num_assets]`                                         | Any     |
| `get_assets`        | Writes the [assets](note.md#assets) of the input note with the specified index into memory starting at the specified address.<br/><br/>**Inputs:** `[dest_ptr, note_index]`<br/>**Outputs:** `[num_assets, dest_ptr, note_index]` | Any     |
| `get_recipient`     | Returns the [recipient](note.md#note-recipient-restricting-consumption) of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[RECIPIENT]`                                            | Any     |
| `get_metadata`      | Returns the [metadata](note.md#metadata) of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[METADATA]`                                                                            | Any     |
| `get_sender`        | Returns the sender of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[sender_id_prefix, sender_id_suffix]`                                                                      | Any     |
| `get_inputs_info`   | Returns the [inputs](note.md#inputs) commitment and length of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[NOTE_INPUTS_COMMITMENT, num_inputs]`                                | Any     |
| `get_script_root`   | Returns the [script root](note.md#script) of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[SCRIPT_ROOT]`                                                                        | Any     |
| `get_serial_number` | Returns the [serial number](note.md#serial-number) of the input note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[SERIAL_NUMBER]`                                                             | Any     |

## Output Note Procedures (`miden::output_note`)

Output note procedures can be used to fetch data on output notes created by the transaction.

| Procedure         | Description                                                                                                                                                                                                      | Context          |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| `create`          | Creates a new output note and returns its index.<br/><br/>**Inputs:** `[tag, aux, note_type, execution_hint, RECIPIENT]`<br/>**Outputs:** `[note_idx]`                                                           | Native & Account |
| `get_assets_info` | Returns the information about assets in the output note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[ASSETS_COMMITMENT, num_assets]`                                         | Any              |
| `get_assets`      | Writes the assets of the output note with the specified index into memory starting at the specified address.<br/><br/>**Inputs:** `[dest_ptr, note_index]`<br/>**Outputs:** `[num_assets, dest_ptr, note_index]` | Any              |
| `add_asset`       | Adds the `ASSET` to the output note specified by the index.<br/><br/>**Inputs:** `[ASSET, note_idx]`<br/>**Outputs:** `[ASSET, note_idx]`                                                                        | Native           |
| `get_recipient`   | Returns the [recipient](note#note-recipient-restricting-consumption) of the output note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[RECIPIENT]`                             | Any              |
| `get_metadata`    | Returns the [metadata](note#metadata) of the output note with the specified index.<br/><br/>**Inputs:** `[note_index]`<br/>**Outputs:** `[METADATA]`                                                             | Any              |

## Note Utility Procedures (`miden::note`)

Note utility procedures can be used to compute the required utility data or write note data to memory.

| Procedure                   | Description                                                                                                                                                                                                             | Context |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| `compute_inputs_commitment` | Computes the commitment to the output note inputs starting at the specified memory address.<br/><br/>**Inputs:** `[inputs_ptr, num_inputs]`<br/>**Outputs:** `[INPUTS_COMMITMENT]`                                      | Any     |
| `get_max_inputs_per_note`   | Returns the max allowed number of input values per note.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[max_inputs_per_note]`                                                                                             | Any     |
| `write_assets_to_memory`    | Writes the assets data stored in the advice map to the memory specified by the provided destination pointer.<br/><br/>**Inputs:** `[ASSETS_COMMITMENT, num_assets, dest_ptr]`<br/>**Outputs:** `[num_assets, dest_ptr]` | Any     |
| `build_recipient_hash`      | Returns the `RECIPIENT` for a specified `SERIAL_NUM`, `SCRIPT_ROOT`, and inputs commitment.<br/><br/>**Inputs:** `[SERIAL_NUM, SCRIPT_ROOT, INPUT_COMMITMENT]`<br/>**Outputs:** `[RECIPIENT]`                           | Any     |
| `build_recipient`           | Builds the recipient hash from note inputs, script root, and serial number.<br/><br/>**Inputs:** `[inputs_ptr, num_inputs, SERIAL_NUM, SCRIPT_ROOT]`<br/>**Outputs:** `[RECIPIENT]`                                     | Any     |
| `extract_sender_from_metadata` | Extracts the sender ID from the provided metadata word.<br/><br/>**Inputs:** `[METADATA]`<br/>**Outputs:** `[sender_id_prefix, sender_id_suffix]` | Any     |

## Transaction Procedures (`miden::tx`)

Transaction procedures manage transaction-level operations including note creation, context switching, and reading transaction metadata.

| Procedure                       | Description                                                                                                                                                                                                    | Context |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| `get_block_number`              | Returns the block number of the transaction reference block.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[num]`                                                                                                | Any     |
| `get_block_commitment`          | Returns the block commitment of the reference block.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[BLOCK_COMMITMENT]`                                                                                           | Any     |
| `get_block_timestamp`           | Returns the timestamp of the reference block for this transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[timestamp]`                                                                                    | Any     |
| `get_input_notes_commitment`    | Returns the input notes commitment hash.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[INPUT_NOTES_COMMITMENT]`                                                                                                 | Any     |
| `get_output_notes_commitment`   | Returns the output notes commitment hash.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[OUTPUT_NOTES_COMMITMENT]`                                                                                               | Any     |
| `get_num_input_notes`           | Returns the total number of input notes consumed by this transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[num_input_notes]`                                                                           | Any     |
| `get_num_output_notes`          | Returns the current number of output notes created in this transaction.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[num_output_notes]`                                                                        | Any     |
| `execute_foreign_procedure`     | Executes the provided procedure against the foreign account.<br/><br/>**Inputs:** `[foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, <inputs>, pad(n)]`<br/>**Outputs:** `[<outputs>]` | Any     |
| `get_expiration_block_delta`    | Returns the transaction expiration delta, or 0 if not set.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[block_height_delta]`                                                                                   | Any     |
| `update_expiration_block_delta` | Updates the transaction expiration delta.<br/><br/>**Inputs:** `[block_height_delta]`<br/>**Outputs:** `[]`                                                                                                    | Any     |

## Faucet Procedures (`miden::faucet`)

Faucet procedures allow reading and writing to faucet accounts to mint and burn assets.

| Procedure                      | Description                                                                                                                                                                | Context                   |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| `create_fungible_asset`        | Creates a fungible asset for the faucet the transaction is being executed against.<br/><br/>**Inputs:** `[amount]`<br/>**Outputs:** `[ASSET]`                              | Faucet                    |
| `create_non_fungible_asset`    | Creates a non-fungible asset for the faucet the transaction is being executed against.<br/><br/>**Inputs:** `[DATA_HASH]`<br/>**Outputs:** `[ASSET]`                       | Faucet                    |
| `mint`                         | Mint an asset from the faucet the transaction is being executed against.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[ASSET]`                                         | Native & Account & Faucet |
| `burn`                         | Burn an asset from the faucet the transaction is being executed against.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[ASSET]`                                         | Native & Account & Faucet |
| `get_total_issuance`           | Returns the total issuance of the fungible faucet the transaction is being executed against.<br/><br/>**Inputs:** `[]`<br/>**Outputs:** `[total_issuance]`                 | Faucet                    |
| `is_non_fungible_asset_issued` | Returns a boolean indicating whether the provided non-fungible asset has been already issued by this faucet.<br/><br/>**Inputs:** `[ASSET]`<br/>**Outputs:** `[is_issued]` | Faucet                    |

## Asset Procedures (`miden::asset`)

Asset procedures provide utilities for creating fungible and non-fungible assets.

| Procedure                  | Description                                                                                                                                                          | Context |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| `build_fungible_asset`     | Builds a fungible asset for the specified fungible faucet and amount.<br/><br/>**Inputs:** `[faucet_id_prefix, faucet_id_suffix, amount]`<br/>**Outputs:** `[ASSET]` | Any     |
| `build_non_fungible_asset` | Builds a non-fungible asset for the specified non-fungible faucet and data hash.<br/><br/>**Inputs:** `[faucet_id_prefix, DATA_HASH]`<br/>**Outputs:** `[ASSET]`     | Any     |
