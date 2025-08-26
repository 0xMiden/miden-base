# Kernel Procedures

The transaction kernel provides a set of procedures that can be invoked by account code, note scripts, and transaction scripts. These procedures are organized into several categories and have limitations on which context they can be called from, which are documented here.

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

The following section details all kernel procedures. Note that the shown input and output stack is expected to be padded to 16 field elements.

## Account Procedures

Account procedures can be used to read and write to account storage, add or remove assets from the vault and fetch or compute commitments. These procedures are more convenient to use with the wrappers in `miden::account`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `account_get_initial_commitment` | `[]` | `[INIT_COMMITMENT]` | Any | Returns the native account commitment at the beginning of the transaction. |
| `account_compute_current_commitment` | `[]` | `[COMMITMENT]` | Any | Computes the account commitment of the current account. |
| `account_compute_delta_commitment` | `[]` | `[DELTA_COMMITMENT]` | Auth | Computes the commitment to the native account's delta. |
| `account_get_id` | `[]` | `[acct_id_prefix, acct_id_suffix]` | Any | Returns the account ID of the current account. |
| `account_get_nonce` | `[]` | `[nonce]` | Any | Returns the nonce of the current account. |
| `account_incr_nonce` | `[]` | `[final_nonce]` | Auth | Increments the account nonce by one and returns the new nonce. |
| `account_get_code_commitment` | `[]` | `[CODE_COMMITMENT]` | Account | Gets the account code commitment of the current account. |
| `account_get_initial_storage_commitment` | `[]` | `[INIT_STORAGE_COMMITMENT]` | Any | Returns the storage commitment of the native account at the beginning of the transaction. |
| `account_compute_storage_commitment` | `[]` | `[STORAGE_COMMITMENT]` | Account | Computes the latest account storage commitment of the current account. |
| `account_get_item` | `[index]` | `[VALUE]` | Account | Gets an item from the account storage. |
| `account_set_item` | `[index, VALUE]` | `[OLD_VALUE]` | Native & Account | Sets an item in the account storage. |
| `account_get_map_item` | `[index, KEY]` | `[VALUE]` | Account | Returns the VALUE located under the specified KEY within the map contained in the given account storage slot. |
| `account_set_map_item` | `[index, KEY, NEW_VALUE]` | `[OLD_MAP_ROOT, OLD_MAP_VALUE]` | Native & Account | Stores NEW_VALUE under the specified KEY within the map contained in the given account storage slot. |
| `account_get_initial_vault_root` | `[]` | `[INIT_VAULT_ROOT]` | Any | Returns the vault root of the native account at the beginning of the transaction. |
| `account_get_vault_root` | `[]` | `[VAULT_ROOT]` | Any | Returns the vault root of the current account. |
| `account_add_asset` | `[ASSET]` | `[ASSET']` | Native & Account | Adds the specified asset to the vault. |
| `account_remove_asset` | `[ASSET]` | `[ASSET]` | Native & Account | Removes the specified asset from the vault. |
| `account_get_balance` | `[faucet_id_prefix, faucet_id_suffix]` | `[balance]` | Any | Returns the balance of the fungible asset associated with the provided faucet_id in the current account's vault. |
| `account_has_non_fungible_asset` | `[ASSET]` | `[has_asset]` | Any | Returns a boolean indicating whether the non-fungible asset is present in the current account's vault. |
| `account_was_procedure_called` | `[PROC_ROOT]` | `[was_called]` | Any | Checks if a procedure has been called during transaction execution. |

## Note Procedures

Note procedures can be used to fetch data from the note that is currently being processed as well as input and output notes.

### Current Note Procedures

Current note procedures can be used to fetch data from the note that is currently being processed. These procedures are more convenient to use with the wrappers in `miden::note`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `note_get_assets_info` | `[]` | `[ASSETS_COMMITMENT, num_assets]` | Note | Returns the information about assets in the input note with the specified index. Panics if a note is not being processed. |
| `note_add_asset` | `[note_idx, ASSET]` | `[note_idx, ASSET]` | Native | Adds the ASSET to the note specified by the index. |
| `note_get_serial_number` | `[]` | `[SERIAL_NUMBER]` | Note | Returns the serial number of the note currently being processed. Panics if no note is being processed. |
| `note_get_inputs_commitment_and_len` | `[]` | `[NOTE_INPUTS_COMMITMENT, num_inputs]` | Note | Returns the current note's inputs commitment and length. |
| `note_get_sender` | `[]` | `[sender_id_prefix, sender_id_suffix]` | Note | Returns the sender of the note currently being processed. Panics if a note is not being processed. |
| `note_get_script_root` | `[]` | `[script_root]` | Note | Returns the script root of the note currently being processed. Panics if no note is being processed. |

### Input Note Procedures

Input note procedures can be used to fetch data on input notes. These procedures are more convenient to use with the wrappers in `miden::input_note`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `input_note_get_assets_info` | `[note_index]` | `[ASSETS_COMMITMENT, num_assets]` | Any | Returns the information about assets in the input note with the specified index. Panics if the note index is greater or equal to the total number of input notes. |
| `input_note_get_recipient` | `[note_index]` | `[RECIPIENT]` | Any | Returns the recipient of the input note with the specified index. Panics if the note index is greater or equal to the total number of input notes. |
| `input_note_get_metadata` | `[note_index]` | `[METADATA]` | Any | Returns the metadata of the input note with the specified index. Panics if the note index is greater or equal to the total number of input notes. |

### Output Note Procedures

Output note procedures can be used to fetch data on output notes. These procedures are more convenient to use with the wrappers in `miden::output_note`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `output_note_get_assets_info` | `[note_index]` | `[ASSETS_COMMITMENT, num_assets]` | Any | Returns the information about assets in the output note with the specified index. Panics if the note index is greater or equal to the total number of output notes. |
| `output_note_get_recipient` | `[note_index]` | `[RECIPIENT]` | Any | Returns the recipient of the output note with the specified index. Panics if the note index is greater or equal to the total number of output notes. |
| `output_note_get_metadata` | `[note_index]` | `[METADATA]` | Any | Returns the metadata of the output note with the specified index. Panics if the note index is greater or equal to the total number of output notes. |

## Transaction Procedures

Transaction procedures manage transaction-level operations including note creation, context switching, and reading or writing transaction metadata. These procedures are more convenient to use with the wrappers in `miden::tx`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `tx_create_note` | `[tag, aux, note_type, execution_hint, RECIPIENT]` | `[note_idx]` | Native & Account | Creates a new note and returns the index of the note. |
| `tx_get_input_notes_commitment` | `[]` | `[INPUT_NOTES_COMMITMENT]` | Any | Returns the input notes commitment. This is computed as a sequential hash of `(NULLIFIER, EMPTY_WORD_OR_NOTE_COMMITMENT)` over all input notes. |
| `tx_get_num_input_notes` | `[]` | `[num_input_notes]` | Any | Returns the total number of input notes consumed by this transaction. |
| `tx_get_output_notes_commitment` | `[]` | `[OUTPUT_NOTES_COMMITMENT]` | Any | Returns the output notes commitment. |
| `tx_get_num_output_notes` | `[]` | `[num_output_notes]` | Any | Returns the current number of output notes created in this transaction. |
| `tx_get_block_commitment` | `[]` | `[BLOCK_COMMITMENT]` | Any | Returns the block commitment of the reference block. |
| `tx_get_block_number` | `[]` | `[num]` | Any | Returns the block number of the transaction reference block at the time of transaction execution. |
| `tx_get_block_timestamp` | `[]` | `[timestamp]` | Any | Returns the block timestamp of the reference block for this transaction. |
| `tx_start_foreign_context` | `[foreign_account_id_prefix, foreign_account_id_suffix]` | `[]` | Any | Starts a foreign account context. |
| `tx_end_foreign_context` | `[]` | `[]` | Any | Ends a foreign account context. |
| `tx_update_expiration_block_num` | `[block_height_delta]` | `[]` | Any | Updates the transaction expiration time delta. |
| `tx_get_expiration_delta` | `[]` | `[block_height_delta]` | Any | Gets the transaction expiration delta. |

## Faucet Procedures

Faucet procedures allow reading and writing to faucet accounts to mint and burn assets. These procedures are more convenient to use with the wrappers in `miden::faucet`.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `faucet_mint_asset` | `[ASSET]` | `[ASSET]` | Native & Account & Faucet | Mint an asset from the faucet the transaction is being executed against. |
| `faucet_burn_asset` | `[ASSET]` | `[ASSET]` | Native & Account & Faucet | Burn an asset from the faucet the transaction is being executed against. |
| `faucet_get_total_fungible_asset_issuance` | `[]` | `[total_issuance]` | Faucet | Returns the total issuance of the fungible faucet the transaction is being executed against. |
| `faucet_is_non_fungible_asset_issued` | `[ASSET]` | `[is_issued]` | Faucet | Returns a boolean indicating whether the provided non-fungible asset has been already issued by this faucet. |

## Kernel Procedure Execution

A kernel procedure is called by providing its `procedure_offset` and `syscall`-ing `exec_kernel_proc`. instead of their MAST root. This has the advantage that kernel procedures can change without breaking callers, since users only commit to the procedure offset rather than its root.

| Procedure | Inputs | Outputs | Context | Description |
| --- | --- | --- | --- | --- |
| `exec_kernel_proc` | `[procedure_offset, <procedure_inputs>, <pad>]` | `[<procedure_outputs>, <pad>]` | Any | Executes a kernel procedure specified by its offset. |
