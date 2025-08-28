# Miden Library Procedures

The Miden library provides a set of high-level procedures that can be invoked by account code, note scripts, and transaction scripts. These procedures wrap the underlying kernel procedures and provide a more convenient interface for common operations. The procedures are organized into modules corresponding to different functional areas.

## Contexts

This describes contexts from which procedures can be called:

- **Any**: Can be called from any context.
- **Account**: Can only be called from native or foreign accounts.
- **Native**: Can only be called when the current account is the native account.
- **Auth**: Can only be called from the authentication procedure. Since it is called on the native account, it implies **Native** and **Account**.
- **Note**: Can only be called from a note script.
- **Faucet**: Can only be called when the current account is a faucet.

If a procedure has multiple context requirements they are combined using `&`. For instance, "Native & Account" means the procedure can only be called when the current account is the native one _and_ only from the account context.

## Procedures

The following sections detail all procedures available in the Miden library, organized by module.

## Account Procedures (`miden::account`)

Account procedures can be used to read and write to account storage, add or remove assets from the vault and fetch or compute commitments. These procedures provide high-level wrappers for kernel operations.

| Procedure | Description |
| --- | --- |
| `get_id` | Returns the account ID of the current account.<br>**Inputs**: `[]`<br>**Outputs**: `[account_id_prefix, account_id_suffix]`<br>**Context**: Any |
| `get_nonce` | Returns the nonce of the current account. Always returns the initial nonce as it can only be incremented in auth procedures.<br>**Inputs**: `[]`<br>**Outputs**: `[nonce]`<br>**Context**: Any |
| `get_initial_commitment` | Returns the native account commitment at the beginning of the transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[INIT_COMMITMENT]`<br>**Context**: Any |
| `compute_current_commitment` | Computes and returns the account commitment from account data stored in memory.<br>**Inputs**: `[]`<br>**Outputs**: `[ACCOUNT_COMMITMENT]`<br>**Context**: Any |
| `compute_delta_commitment` | Computes the commitment to the native account's delta. Can only be called from auth procedures.<br>**Inputs**: `[]`<br>**Outputs**: `[DELTA_COMMITMENT]`<br>**Context**: Auth |
| `incr_nonce` | Increments the account nonce by one and returns the new nonce. Can only be called from auth procedures.<br>**Inputs**: `[]`<br>**Outputs**: `[final_nonce]`<br>**Context**: Auth |
| `get_item` | Gets an item from the account storage.<br>**Inputs**: `[index]`<br>**Outputs**: `[VALUE]`<br>**Context**: Account |
| `set_item` | Sets an item in the account storage.<br>**Inputs**: `[index, VALUE]`<br>**Outputs**: `[OLD_VALUE]`<br>**Context**: Native & Account |
| `get_map_item` | Returns the VALUE located under the specified KEY within the map contained in the given account storage slot.<br>**Inputs**: `[index, KEY]`<br>**Outputs**: `[VALUE]`<br>**Context**: Account |
| `set_map_item` | Sets VALUE under the specified KEY within the map contained in the given account storage slot.<br>**Inputs**: `[index, KEY, VALUE]`<br>**Outputs**: `[OLD_MAP_ROOT, OLD_MAP_VALUE]`<br>**Context**: Native & Account |
| `get_code_commitment` | Gets the account code commitment of the current account.<br>**Inputs**: `[]`<br>**Outputs**: `[CODE_COMMITMENT]`<br>**Context**: Account |
| `get_initial_storage_commitment` | Returns the storage commitment of the native account at the beginning of the transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[INIT_STORAGE_COMMITMENT]`<br>**Context**: Any |
| `compute_storage_commitment` | Computes the latest account storage commitment of the current account.<br>**Inputs**: `[]`<br>**Outputs**: `[STORAGE_COMMITMENT]`<br>**Context**: Account |
| `get_balance` | Returns the balance of the fungible asset associated with the provided faucet_id in the current account's vault.<br>**Inputs**: `[faucet_id_prefix, faucet_id_suffix]`<br>**Outputs**: `[balance]`<br>**Context**: Any |
| `has_non_fungible_asset` | Returns a boolean indicating whether the non-fungible asset is present in the current account's vault.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[has_asset]`<br>**Context**: Any |
| `add_asset` | Adds the specified asset to the vault. For fungible assets, returns the total after addition.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[ASSET']`<br>**Context**: Native & Account |
| `remove_asset` | Removes the specified asset from the vault.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[ASSET]`<br>**Context**: Native & Account |
| `get_initial_vault_root` | Returns the vault root of the native account at the beginning of the transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[INIT_VAULT_ROOT]`<br>**Context**: Any |
| `get_vault_root` | Returns the vault root of the current account.<br>**Inputs**: `[]`<br>**Outputs**: `[VAULT_ROOT]`<br>**Context**: Any |
| `was_procedure_called` | Checks if a procedure has been called during transaction execution.<br>**Inputs**: `[PROC_ROOT]`<br>**Outputs**: `[was_called]`<br>**Context**: Any |

## Note Procedures (`miden::note`)

Note procedures can be used to fetch data from the note that is currently being processed and manipulate note assets.

| Procedure | Description |
| --- | --- |
| `get_assets` | Writes the assets of the currently executing note into memory starting at the specified address.<br>**Inputs**: `[dest_ptr]`<br>**Outputs**: `[num_assets, dest_ptr]`<br>**Context**: Note |
| `get_inputs` | Loads the note's inputs to the specified memory address.<br>**Inputs**: `[dest_ptr]`<br>**Outputs**: `[num_inputs, dest_ptr]`<br>**Context**: Note |
| `get_sender` | Returns the sender of the note currently being processed.<br>**Inputs**: `[]`<br>**Outputs**: `[sender_id_prefix, sender_id_suffix]`<br>**Context**: Note |
| `get_serial_number` | Returns the serial number of the note currently being processed.<br>**Inputs**: `[]`<br>**Outputs**: `[SERIAL_NUMBER]`<br>**Context**: Note |
| `get_script_root` | Returns the script root of the note currently being processed.<br>**Inputs**: `[]`<br>**Outputs**: `[SCRIPT_ROOT]`<br>**Context**: Note |
| `compute_inputs_commitment` | Computes commitment to the note inputs starting at the specified memory address.<br>**Inputs**: `[inputs_ptr, num_inputs]`<br>**Outputs**: `[COMMITMENT]`<br>**Context**: Any |
| `add_assets_to_account` | Adds all assets from the currently executing note to the account vault.<br>**Inputs**: `[]`<br>**Outputs**: `[]`<br>**Context**: Note |

## Input Note Procedures (`miden::input_note`)

Input note procedures can be used to fetch data on input notes consumed by the transaction.

| Procedure | Description |
| --- | --- |
| `get_assets_info` | Returns the information about assets in the input note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[ASSETS_COMMITMENT, num_assets]`<br>**Context**: Any |
| `get_assets` | Writes the assets of the input note with the specified index into memory starting at the specified address.<br>**Inputs**: `[dest_ptr, note_index]`<br>**Outputs**: `[num_assets, dest_ptr, note_index]`<br>**Context**: Any |
| `get_recipient` | Returns the recipient of the input note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[RECIPIENT]`<br>**Context**: Any |
| `get_metadata` | Returns the metadata of the input note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[METADATA]`<br>**Context**: Any |

## Output Note Procedures (`miden::output_note`)

Output note procedures can be used to fetch data on output notes created by the transaction.

| Procedure | Description |
| --- | --- |
| `get_assets_info` | Returns the information about assets in the output note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[ASSETS_COMMITMENT, num_assets]`<br>**Context**: Any |
| `get_assets` | Writes the assets of the output note with the specified index into memory starting at the specified address.<br>**Inputs**: `[dest_ptr, note_index]`<br>**Outputs**: `[num_assets, dest_ptr, note_index]`<br>**Context**: Any |
| `get_recipient` | Returns the recipient of the output note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[RECIPIENT]`<br>**Context**: Any |
| `get_metadata` | Returns the metadata of the output note with the specified index.<br>**Inputs**: `[note_index]`<br>**Outputs**: `[METADATA]`<br>**Context**: Any |

## Transaction Procedures (`miden::tx`)

Transaction procedures manage transaction-level operations including note creation, context switching, and reading transaction metadata.

| Procedure | Description |
| --- | --- |
| `get_block_number` | Returns the block number of the transaction reference block.<br>**Inputs**: `[]`<br>**Outputs**: `[num]`<br>**Context**: Any |
| `get_block_commitment` | Returns the block commitment of the reference block.<br>**Inputs**: `[]`<br>**Outputs**: `[BLOCK_COMMITMENT]`<br>**Context**: Any |
| `get_block_timestamp` | Returns the timestamp of the reference block for this transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[timestamp]`<br>**Context**: Any |
| `get_input_notes_commitment` | Returns the input notes commitment hash.<br>**Inputs**: `[]`<br>**Outputs**: `[INPUT_NOTES_COMMITMENT]`<br>**Context**: Any |
| `get_output_notes_commitment` | Returns the output notes commitment hash.<br>**Inputs**: `[]`<br>**Outputs**: `[OUTPUT_NOTES_COMMITMENT]`<br>**Context**: Any |
| `get_num_input_notes` | Returns the total number of input notes consumed by this transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[num_input_notes]`<br>**Context**: Any |
| `get_num_output_notes` | Returns the current number of output notes created in this transaction.<br>**Inputs**: `[]`<br>**Outputs**: `[num_output_notes]`<br>**Context**: Any |
| `create_note` | Creates a new note and returns the index of the note.<br>**Inputs**: `[tag, aux, note_type, execution_hint, RECIPIENT]`<br>**Outputs**: `[note_idx]`<br>**Context**: Native & Account |
| `add_asset_to_note` | Adds the ASSET to the note specified by the index.<br>**Inputs**: `[ASSET, note_idx]`<br>**Outputs**: `[ASSET, note_idx]`<br>**Context**: Native |
| `build_recipient_hash` | Returns the RECIPIENT for a specified SERIAL_NUM, SCRIPT_ROOT, and inputs commitment.<br>**Inputs**: `[SERIAL_NUM, SCRIPT_ROOT, INPUT_COMMITMENT]`<br>**Outputs**: `[RECIPIENT]`<br>**Context**: Any |
| `execute_foreign_procedure` | Executes the provided procedure against the foreign account.<br>**Inputs**: `[foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, <inputs>, pad(n)]`<br>**Outputs**: `[<outputs>]`<br>**Context**: Any |
| `update_expiration_block_delta` | Updates the transaction expiration delta.<br>**Inputs**: `[block_height_delta]`<br>**Outputs**: `[]`<br>**Context**: Any |
| `get_expiration_block_delta` | Returns the transaction expiration delta, or 0 if not set.<br>**Inputs**: `[]`<br>**Outputs**: `[block_height_delta]`<br>**Context**: Any |

## Faucet Procedures (`miden::faucet`)

Faucet procedures allow reading and writing to faucet accounts to mint and burn assets.

| Procedure | Description |
| --- | --- |
| `mint` | Mint an asset from the faucet the transaction is being executed against.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[ASSET]`<br>**Context**: Native & Account & Faucet |
| `burn` | Burn an asset from the faucet the transaction is being executed against.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[ASSET]`<br>**Context**: Native & Account & Faucet |
| `get_total_issuance` | Returns the total issuance of the fungible faucet the transaction is being executed against.<br>**Inputs**: `[]`<br>**Outputs**: `[total_issuance]`<br>**Context**: Faucet |
| `is_non_fungible_asset_issued` | Returns a boolean indicating whether the provided non-fungible asset has been already issued by this faucet.<br>**Inputs**: `[ASSET]`<br>**Outputs**: `[is_issued]`<br>**Context**: Faucet |

## Asset Procedures (`miden::asset`)

Asset procedures provide utilities for creating fungible and non-fungible assets.

| Procedure | Description |
| --- | --- |
| `build_fungible_asset` | Builds a fungible asset for the specified fungible faucet and amount.<br>**Inputs**: `[faucet_id_prefix, faucet_id_suffix, amount]`<br>**Outputs**: `[ASSET]`<br>**Context**: Any |
| `create_fungible_asset` | Creates a fungible asset for the faucet the transaction is being executed against.<br>**Inputs**: `[amount]`<br>**Outputs**: `[ASSET]`<br>**Context**: Faucet |
| `build_non_fungible_asset` | Builds a non-fungible asset for the specified non-fungible faucet and data hash.<br>**Inputs**: `[faucet_id_prefix, DATA_HASH]`<br>**Outputs**: `[ASSET]`<br>**Context**: Any |
| `create_non_fungible_asset` | Creates a non-fungible asset for the faucet the transaction is being executed against.<br>**Inputs**: `[DATA_HASH]`<br>**Outputs**: `[ASSET]`<br>**Context**: Faucet |

## Implementation Notes

All procedures in the Miden library are implemented as wrappers around the underlying kernel procedures. They handle the necessary stack padding and cleanup operations required by the kernel interface, providing a more convenient API for developers.

The procedures maintain the same security and context restrictions as the underlying kernel procedures. When invoking these procedures, ensure that the calling context matches the specified requirements.
