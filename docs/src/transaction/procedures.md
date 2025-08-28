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

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `get_id` | `[]` | `[account_id_prefix, account_id_suffix]` | Any | Returns the account ID of the current account. |
| `get_nonce` | `[]` | `[nonce]` | Any | Returns the nonce of the current account. Always returns the initial nonce as it can only be incremented in auth procedures. |
| `get_initial_commitment` | `[]` | `[INIT_COMMITMENT]` | Any | Returns the native account commitment at the beginning of the transaction. |
| `compute_current_commitment` | `[]` | `[ACCOUNT_COMMITMENT]` | Any | Computes and returns the account commitment from account data stored in memory. |
| `compute_delta_commitment` | `[]` | `[DELTA_COMMITMENT]` | Auth | Computes the commitment to the native account's delta. Can only be called from auth procedures. |
| `incr_nonce` | `[]` | `[final_nonce]` | Auth | Increments the account nonce by one and returns the new nonce. Can only be called from auth procedures. |
| `get_item` | `[index]` | `[VALUE]` | Account | Gets an item from the account storage. |
| `set_item` | `[index, VALUE]` | `[OLD_VALUE]` | Native & Account | Sets an item in the account storage. |
| `get_map_item` | `[index, KEY]` | `[VALUE]` | Account | Returns the VALUE located under the specified KEY within the map contained in the given account storage slot. |
| `set_map_item` | `[index, KEY, VALUE]` | `[OLD_MAP_ROOT, OLD_MAP_VALUE]` | Native & Account | Sets VALUE under the specified KEY within the map contained in the given account storage slot. |
| `get_code_commitment` | `[]` | `[CODE_COMMITMENT]` | Account | Gets the account code commitment of the current account. |
| `get_initial_storage_commitment` | `[]` | `[INIT_STORAGE_COMMITMENT]` | Any | Returns the storage commitment of the native account at the beginning of the transaction. |
| `compute_storage_commitment` | `[]` | `[STORAGE_COMMITMENT]` | Account | Computes the latest account storage commitment of the current account. |
| `get_balance` | `[faucet_id_prefix, faucet_id_suffix]` | `[balance]` | Any | Returns the balance of the fungible asset associated with the provided faucet_id in the current account's vault. |
| `has_non_fungible_asset` | `[ASSET]` | `[has_asset]` | Any | Returns a boolean indicating whether the non-fungible asset is present in the current account's vault. |
| `add_asset` | `[ASSET]` | `[ASSET']` | Native & Account | Adds the specified asset to the vault. For fungible assets, returns the total after addition. |
| `remove_asset` | `[ASSET]` | `[ASSET]` | Native & Account | Removes the specified asset from the vault. |
| `get_initial_vault_root` | `[]` | `[INIT_VAULT_ROOT]` | Any | Returns the vault root of the native account at the beginning of the transaction. |
| `get_vault_root` | `[]` | `[VAULT_ROOT]` | Any | Returns the vault root of the current account. |
| `was_procedure_called` | `[PROC_ROOT]` | `[was_called]` | Any | Checks if a procedure has been called during transaction execution. |

## Note Procedures (`miden::note`)

Note procedures can be used to fetch data from the note that is currently being processed and manipulate note assets.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `get_assets` | `[dest_ptr]` | `[num_assets, dest_ptr]` | Note | Writes the assets of the currently executing note into memory starting at the specified address. |
| `get_inputs` | `[dest_ptr]` | `[num_inputs, dest_ptr]` | Note | Loads the note's inputs to the specified memory address. |
| `get_sender` | `[]` | `[sender_id_prefix, sender_id_suffix]` | Note | Returns the sender of the note currently being processed. |
| `get_serial_number` | `[]` | `[SERIAL_NUMBER]` | Note | Returns the serial number of the note currently being processed. |
| `get_script_root` | `[]` | `[SCRIPT_ROOT]` | Note | Returns the script root of the note currently being processed. |
| `compute_inputs_commitment` | `[inputs_ptr, num_inputs]` | `[COMMITMENT]` | Any | Computes commitment to the note inputs starting at the specified memory address. |
| `add_assets_to_account` | `[]` | `[]` | Note | Adds all assets from the currently executing note to the account vault. |

## Input Note Procedures (`miden::input_note`)

Input note procedures can be used to fetch data on input notes consumed by the transaction.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `get_assets_info` | `[note_index]` | `[ASSETS_COMMITMENT, num_assets]` | Any | Returns the information about assets in the input note with the specified index. |
| `get_assets` | `[dest_ptr, note_index]` | `[num_assets, dest_ptr, note_index]` | Any | Writes the assets of the input note with the specified index into memory starting at the specified address. |
| `get_recipient` | `[note_index]` | `[RECIPIENT]` | Any | Returns the recipient of the input note with the specified index. |
| `get_metadata` | `[note_index]` | `[METADATA]` | Any | Returns the metadata of the input note with the specified index. |

## Output Note Procedures (`miden::output_note`)

Output note procedures can be used to fetch data on output notes created by the transaction.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `get_assets_info` | `[note_index]` | `[ASSETS_COMMITMENT, num_assets]` | Any | Returns the information about assets in the output note with the specified index. |
| `get_assets` | `[dest_ptr, note_index]` | `[num_assets, dest_ptr, note_index]` | Any | Writes the assets of the output note with the specified index into memory starting at the specified address. |
| `get_recipient` | `[note_index]` | `[RECIPIENT]` | Any | Returns the recipient of the output note with the specified index. |
| `get_metadata` | `[note_index]` | `[METADATA]` | Any | Returns the metadata of the output note with the specified index. |

## Transaction Procedures (`miden::tx`)

Transaction procedures manage transaction-level operations including note creation, context switching, and reading transaction metadata.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `get_block_number` | `[]` | `[num]` | Any | Returns the block number of the transaction reference block. |
| `get_block_commitment` | `[]` | `[BLOCK_COMMITMENT]` | Any | Returns the block commitment of the reference block. |
| `get_block_timestamp` | `[]` | `[timestamp]` | Any | Returns the timestamp of the reference block for this transaction. |
| `get_input_notes_commitment` | `[]` | `[INPUT_NOTES_COMMITMENT]` | Any | Returns the input notes commitment hash. |
| `get_output_notes_commitment` | `[]` | `[OUTPUT_NOTES_COMMITMENT]` | Any | Returns the output notes commitment hash. |
| `get_num_input_notes` | `[]` | `[num_input_notes]` | Any | Returns the total number of input notes consumed by this transaction. |
| `get_num_output_notes` | `[]` | `[num_output_notes]` | Any | Returns the current number of output notes created in this transaction. |
| `create_note` | `[tag, aux, note_type, execution_hint, RECIPIENT]` | `[note_idx]` | Native & Account | Creates a new note and returns the index of the note. |
| `add_asset_to_note` | `[ASSET, note_idx]` | `[ASSET, note_idx]` | Native | Adds the ASSET to the note specified by the index. |
| `build_recipient_hash` | `[SERIAL_NUM, SCRIPT_ROOT, INPUT_COMMITMENT]` | `[RECIPIENT]` | Any | Returns the RECIPIENT for a specified SERIAL_NUM, SCRIPT_ROOT, and inputs commitment. |
| `execute_foreign_procedure` | `[foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, <inputs>, pad(n)]` | `[<outputs>]` | Any | Executes the provided procedure against the foreign account. |
| `update_expiration_block_delta` | `[block_height_delta]` | `[]` | Any | Updates the transaction expiration delta. |
| `get_expiration_block_delta` | `[]` | `[block_height_delta]` | Any | Returns the transaction expiration delta, or 0 if not set. |

## Faucet Procedures (`miden::faucet`)

Faucet procedures allow reading and writing to faucet accounts to mint and burn assets.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `mint` | `[ASSET]` | `[ASSET]` | Native & Account & Faucet | Mint an asset from the faucet the transaction is being executed against. |
| `burn` | `[ASSET]` | `[ASSET]` | Native & Account & Faucet | Burn an asset from the faucet the transaction is being executed against. |
| `get_total_issuance` | `[]` | `[total_issuance]` | Faucet | Returns the total issuance of the fungible faucet the transaction is being executed against. |
| `is_non_fungible_asset_issued` | `[ASSET]` | `[is_issued]` | Faucet | Returns a boolean indicating whether the provided non-fungible asset has been already issued by this faucet. |

## Asset Procedures (`miden::asset`)

Asset procedures provide utilities for creating fungible and non-fungible assets.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `build_fungible_asset` | `[faucet_id_prefix, faucet_id_suffix, amount]` | `[ASSET]` | Any | Builds a fungible asset for the specified fungible faucet and amount. |
| `create_fungible_asset` | `[amount]` | `[ASSET]` | Faucet | Creates a fungible asset for the faucet the transaction is being executed against. |
| `build_non_fungible_asset` | `[faucet_id_prefix, DATA_HASH]` | `[ASSET]` | Any | Builds a non-fungible asset for the specified non-fungible faucet and data hash. |
| `create_non_fungible_asset` | `[DATA_HASH]` | `[ASSET]` | Faucet | Creates a non-fungible asset for the faucet the transaction is being executed against. |

## Implementation Notes

All procedures in the Miden library are implemented as wrappers around the underlying kernel procedures. They handle the necessary stack padding and cleanup operations required by the kernel interface, providing a more convenient API for developers.

The procedures maintain the same security and context restrictions as the underlying kernel procedures. When invoking these procedures, ensure that the calling context matches the specified requirements.
