use alloc::vec::Vec;

use miden_objects::{
    Digest, EMPTY_WORD, Felt, FieldElement, TransactionInputError, WORD_SIZE, Word, ZERO,
    account::{AccountHeader, PartialAccount},
    block::AccountWitness,
    transaction::{
        InputNote, PartialBlockchain, TransactionArgs, TransactionInputs, TransactionScript,
    },
    vm::AdviceInputs,
};

use super::TransactionKernel;

// ADVICE INPUTS
// ================================================================================================

/// Extends the provided advice inputs with the data required for executing a transaction with the
/// specified inputs.
///
/// This includes the initial account, an optional account seed (required for new accounts), and
/// the input note data, including core note data + authentication paths all the way to the root
/// of one of partial blockchain peaks.
pub(super) fn extend_advice_inputs(
    tx_inputs: &TransactionInputs,
    tx_args: &TransactionArgs,
    advice_inputs: &mut AdviceInputs,
) -> Result<(), TransactionInputError> {
    // TODO: remove this value and use a user input instead
    let kernel_version = 0;

    build_advice_stack(tx_inputs, tx_args.tx_script(), advice_inputs, kernel_version);

    // build the advice map and Merkle store for relevant components
    add_kernel_commitments_to_advice_inputs(advice_inputs, kernel_version);
    add_partial_blockchain_to_advice_inputs(tx_inputs.block_chain(), advice_inputs);
    add_input_notes_to_advice_inputs(tx_inputs, tx_args, advice_inputs)?;

    // inject account data
    let partial_account = PartialAccount::from(tx_inputs.account());
    add_account_to_advice_inputs(
        &partial_account,
        AccountInputsType::Native(tx_inputs.account_seed()),
        advice_inputs,
    )?;

    // inject foreign account data
    for account_inputs in tx_args.foreign_account_inputs() {
        add_account_to_advice_inputs(
            account_inputs.account(),
            AccountInputsType::Foreign,
            advice_inputs,
        )?;
        add_account_witness_to_advice_inputs(account_inputs.witness(), advice_inputs)?;
    }

    advice_inputs.extend(tx_args.advice_inputs().clone());
    Ok(())
}

// ADVICE STACK BUILDER
// ------------------------------------------------------------------------------------------------

/// Extend the advice stack with the transaction inputs.
///
/// The following data is pushed to the advice stack:
///
/// [
///     PARENT_BLOCK_COMMITMENT,
///     PARTIAL_BLOCKCHAIN_COMMITMENT,
///     ACCOUNT_ROOT,
///     NULLIFIER_ROOT,
///     TX_COMMITMENT,
///     TX_KERNEL_COMMITMENT
///     PROOF_COMMITMENT,
///     [block_num, version, timestamp, 0],
///     NOTE_ROOT,
///     kernel_version
///     [account_id, 0, 0, account_nonce],
///     ACCOUNT_VAULT_ROOT,
///     ACCOUNT_STORAGE_COMMITMENT,
///     ACCOUNT_CODE_COMMITMENT,
///     number_of_input_notes,
///     TX_SCRIPT_ROOT,
///     TX_SCRIPT_ARGS_KEY,
/// ]
fn build_advice_stack(
    tx_inputs: &TransactionInputs,
    tx_script: Option<&TransactionScript>,
    inputs: &mut AdviceInputs,
    kernel_version: u8,
) {
    let header = tx_inputs.block_header();

    // push block header info into the stack
    // Note: keep in sync with the `prologue::process_block_data` kernel procedure
    inputs.extend_stack(header.prev_block_commitment());
    inputs.extend_stack(header.chain_commitment());
    inputs.extend_stack(header.account_root());
    inputs.extend_stack(header.nullifier_root());
    inputs.extend_stack(header.tx_commitment());
    inputs.extend_stack(header.tx_kernel_commitment());
    inputs.extend_stack(header.proof_commitment());
    inputs.extend_stack([
        header.block_num().into(),
        header.version().into(),
        header.timestamp().into(),
        ZERO,
    ]);
    inputs.extend_stack(header.note_root());

    // push the version of the kernel which will be used for this transaction
    // Note: keep in sync with the `prologue::process_kernel_data` kernel procedure
    inputs.extend_stack([Felt::from(kernel_version)]);

    // push core account items onto the stack
    // Note: keep in sync with the `prologue::process_account_data` kernel procedure
    let account = tx_inputs.account();
    inputs.extend_stack([
        account.id().suffix(),
        account.id().prefix().as_felt(),
        ZERO,
        account.nonce(),
    ]);
    inputs.extend_stack(account.vault().root());
    inputs.extend_stack(account.storage().commitment());
    inputs.extend_stack(account.code().commitment());

    // push the number of input notes onto the stack
    inputs.extend_stack([Felt::from(tx_inputs.input_notes().num_notes() as u32)]);

    // push the transaction script root and transaction args key onto the stack
    // Note: keep in sync with the `prologue::process_tx_script_data` kernel procedure
    inputs.extend_stack(tx_script.map_or(Word::default(), |script| *script.root()));
    inputs.extend_stack(
        tx_script.map_or(Word::default(), |script| *script.args_key().unwrap_or_default()),
    );
}

// PARTIAL BLOCKCHAIN INJECTOR
// ------------------------------------------------------------------------------------------------

/// Inserts the partial blockchain data into the provided advice inputs.
///
/// Inserts the following items into the Merkle store:
/// - Inner nodes of all authentication paths contained in the partial blockchain.
///
/// Inserts the following data to the advice map:
///
/// > {MMR_ROOT: [[num_blocks, 0, 0, 0], PEAK_1, ..., PEAK_N]}
///
/// Where:
/// - MMR_ROOT, is the sequential hash of the padded MMR peaks
/// - num_blocks, is the number of blocks in the MMR.
/// - PEAK_1 .. PEAK_N, are the MMR peaks.
fn add_partial_blockchain_to_advice_inputs(mmr: &PartialBlockchain, inputs: &mut AdviceInputs) {
    // NOTE: keep this code in sync with the `prologue::process_chain_data` kernel procedure

    // add authentication paths from the MMR to the Merkle store
    inputs.extend_merkle_store(mmr.inner_nodes());

    // insert MMR peaks info into the advice map
    let peaks = mmr.peaks();
    let mut elements = vec![Felt::new(peaks.num_leaves() as u64), ZERO, ZERO, ZERO];
    elements.extend(peaks.flatten_and_pad_peaks());
    inputs.extend_map([(peaks.hash_peaks(), elements)]);
}

// ACCOUNT DATA INJECTOR
// ------------------------------------------------------------------------------------------------

/// Describes the type of account inputs that should be injected to the advice map:
///
/// - For a native account that is new, a seed must be provided and an account_id |-> [SEED] is
///   injected. For an existing native account, no extra inputs need to be provided.
/// - For foreign accounts, account_id |-> [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT,
///   CODE_COMMITMENT] is injected
enum AccountInputsType {
    /// Inputs for the native (executor) account that is new. The inner value is the seed of the
    /// new account.
    Native(Option<Word>),
    /// Inputs for a foreign account
    Foreign,
}

/// Inserts account data into the provided advice inputs.
///
/// Inserts the following items into the Merkle store:
/// - The Merkle nodes associated with the account vault tree.
/// - If present, the Merkle nodes associated with the account storage maps.
///
/// Inserts the following entries into the advice map:
/// - The account storage commitment |-> storage slots and types vector.
/// - The account code commitment |-> procedures vector.
/// - The leaf hash |-> (key, value), for all leaves of the partial vault.
/// - If present, the Merkle leaves associated with the account storage maps.
/// - account_id |-> account_seed, when `account_inputs_type` is [`AccountInputsType::NativeNew`].
/// - account_id |-> [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT, CODE_COMMITMENT] when
///   `account_inputs_type` is [`AccountInputsType::Foreign`].
fn add_account_to_advice_inputs(
    account: &PartialAccount,
    account_inputs_type: AccountInputsType,
    advice_inputs: &mut AdviceInputs,
) -> Result<(), TransactionInputError> {
    let storage_header = account.storage().header();
    let account_code = account.code();
    let storage_proofs = account.storage().storage_map_proofs();
    let account_id = account.id();

    let account_header = AccountHeader::from(account);

    // --- account storage ----------------------------------------------------

    advice_inputs.extend_map([
        // STORAGE_COMMITMENT -> [[STORAGE_SLOT_DATA]]
        (storage_header.compute_commitment(), storage_header.as_elements()),
    ]);

    // load merkle nodes for storage maps
    for proof in storage_proofs {
        // Extend the merkle store and map with the storage maps
        advice_inputs.extend_merkle_store(
            proof
                .path()
                .inner_nodes(proof.leaf().index().value(), proof.leaf().hash())
                .map_err(|err| {
                    TransactionInputError::InvalidMerklePath(
                        format!("foreign account ID {} storage proof", account_id).into(),
                        err,
                    )
                })?,
        );
        // Populate advice map with Sparse Merkle Tree leaf nodes
        advice_inputs
            .extend_map(core::iter::once((proof.leaf().hash(), proof.leaf().to_elements())));
    }

    // --- account code ----------------------------------------------------

    advice_inputs.extend_map([
        // CODE_COMMITMENT -> [[ACCOUNT_PROCEDURE_DATA]]
        (account_code.commitment(), account_code.as_elements()),
    ]);

    // --- account vault ----------------------------------------------------

    advice_inputs.extend_merkle_store(account.vault().inner_nodes());

    // populate advice map with Sparse Merkle Tree leaf nodes
    advice_inputs
        .extend_map(account.vault().leaves().map(|leaf| (leaf.hash(), leaf.to_elements())));

    // --- account state ----------------------------------------------------

    let account_id_key: Digest =
        Digest::from([account_id.suffix(), account_id.prefix().as_felt(), ZERO, ZERO]);

    // if a seed was provided, extend the map; otherwise inject the state
    match account_inputs_type {
        AccountInputsType::Native(Some(seed)) => advice_inputs.extend_map([(
            // ACCOUNT_ID -> ACCOUNT_SEED
            account_id_key,
            seed.to_vec(),
        )]),
        AccountInputsType::Foreign =>
        // Note: keep in sync with the start_foreign_context kernel procedure
        {
            advice_inputs.extend_map([(
                // ACCOUNT_ID -> [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT, CODE_COMMITMENT]
                account_id_key,
                account_header.as_elements(),
            )])
        },
        AccountInputsType::Native(None) => {},
    }

    Ok(())
}

fn add_account_witness_to_advice_inputs(
    witness: &AccountWitness,
    advice_inputs: &mut AdviceInputs,
) -> Result<(), TransactionInputError> {
    let account_leaf = witness.leaf();
    let account_leaf_hash = account_leaf.hash();

    // populate advice map with the account's leaf
    advice_inputs.extend_map([(account_leaf_hash, account_leaf.to_elements())]);
    // extend the merkle store and map with account witnesses merkle path
    advice_inputs.extend_merkle_store(witness.inner_nodes());
    Ok(())
}

// INPUT NOTE INJECTOR
// ------------------------------------------------------------------------------------------------

/// Populates the advice inputs for all input notes.
///
/// The advice provider is populated with:
///
/// - For each note:
///     - The note's details (serial number, script root, and its input / assets commitment).
///     - The note's private arguments.
///     - The note's public metadata.
///     - The note's public inputs data. Prefixed by its length and padded to an even word length.
///     - The note's asset padded. Prefixed by its length and padded to an even word length.
///     - For authenticated notes (determined by the `is_authenticated` flag):
///         - The note's authentication path against its block's note tree.
///         - The block number, sub commitment, note root.
///         - The note's position in the note tree
///
/// The data above is processed by `prologue::process_input_notes_data`.
fn add_input_notes_to_advice_inputs(
    tx_inputs: &TransactionInputs,
    tx_args: &TransactionArgs,
    inputs: &mut AdviceInputs,
) -> Result<(), TransactionInputError> {
    // if there are no input notes, nothing is added to the advice inputs
    if tx_inputs.input_notes().is_empty() {
        return Ok(());
    }

    let mut note_data = Vec::new();
    for input_note in tx_inputs.input_notes().iter() {
        let note = input_note.note();
        let assets = note.assets();
        let recipient = note.recipient();
        let note_arg = tx_args.get_note_args(note.id()).unwrap_or(&EMPTY_WORD);

        // NOTE: keep map in sync with the `note::get_inputs` API procedure
        inputs.extend_map([(
            recipient.inputs().commitment(),
            recipient.inputs().format_for_advice(),
        )]);

        inputs.extend_map([(assets.commitment(), assets.to_padded_assets())]);

        // NOTE: keep in sync with the `prologue::process_input_note_details` kernel procedure
        note_data.extend(recipient.serial_num());
        note_data.extend(*recipient.script().root());
        note_data.extend(*recipient.inputs().commitment());
        note_data.extend(*assets.commitment());

        // NOTE: keep in sync with the `prologue::process_note_args_and_metadata` kernel procedure
        note_data.extend(Word::from(*note_arg));
        note_data.extend(Word::from(note.metadata()));

        // NOTE: keep in sync with the `prologue::process_note_inputs_length` kernel procedure
        note_data.push(recipient.inputs().num_values().into());

        // NOTE: keep in sync with the `prologue::process_note_assets` kernel procedure
        note_data.push((assets.num_assets() as u32).into());
        note_data.extend(assets.to_padded_assets());

        // insert note authentication path nodes into the Merkle store
        match input_note {
            InputNote::Authenticated { note, proof } => {
                let block_num = proof.location().block_num();
                let note_block_header = if block_num == tx_inputs.block_header().block_num() {
                    tx_inputs.block_header()
                } else {
                    tx_inputs
                        .block_chain()
                        .get_block(block_num)
                        .expect("block not found in partial blockchain")
                };

                // NOTE: keep in sync with the `prologue::process_input_note` kernel procedure
                // Push the `is_authenticated` flag
                note_data.push(Felt::ONE);

                // NOTE: keep in sync with the `prologue::authenticate_note` kernel procedure
                inputs.extend_merkle_store(
                    proof
                        .note_path()
                        .inner_nodes(
                            proof.location().node_index_in_block().into(),
                            note.commitment(),
                        )
                        .map_err(|err| {
                            TransactionInputError::InvalidMerklePath(
                                format!("input note ID {}", note.id()).into(),
                                err,
                            )
                        })?,
                );
                note_data.push(proof.location().block_num().into());
                note_data.extend(note_block_header.sub_commitment());
                note_data.extend(note_block_header.note_root());
                note_data.push(proof.location().node_index_in_block().into());
            },
            InputNote::Unauthenticated { .. } => {
                // NOTE: keep in sync with the `prologue::process_input_note` kernel procedure
                // Push the `is_authenticated` flag
                note_data.push(Felt::ZERO);
            },
        }
    }

    // NOTE: keep map in sync with the `prologue::process_input_notes_data` kernel procedure
    inputs.extend_map([(tx_inputs.input_notes().commitment(), note_data)]);
    Ok(())
}

// KERNEL COMMITMENTS INJECTOR
// ------------------------------------------------------------------------------------------------

/// Inserts kernel commitments and hashes of their procedures into the provided advice inputs.
///
/// Inserts the following entries into the advice map:
/// - The accumulative hash of all kernels |-> array of each kernel commitment.
/// - The hash of the selected kernel |-> array of the kernel's procedure roots.
pub fn add_kernel_commitments_to_advice_inputs(inputs: &mut AdviceInputs, kernel_version: u8) {
    let mut kernel_commitments: Vec<Felt> =
        Vec::with_capacity(TransactionKernel::NUM_VERSIONS * WORD_SIZE);
    for version in 0..TransactionKernel::NUM_VERSIONS {
        kernel_commitments
            .extend_from_slice(TransactionKernel::commitment(version as u8).as_elements());
    }

    // insert the selected kernel commitment with its procedure roots into the advice map
    inputs.extend_map([(
        Digest::new(
            kernel_commitments[kernel_version as usize..kernel_version as usize + WORD_SIZE]
                .try_into()
                .expect("invalid kernel offset"),
        ),
        TransactionKernel::procedures_as_elements(kernel_version),
    )]);

    // insert kernels root with kernel commitments into the advice map
    inputs.extend_map([(TransactionKernel::kernel_commitment(), kernel_commitments)]);
}
