use alloc::vec::Vec;

use miden_objects::account::{AccountHeader, AccountId, PartialAccount};
use miden_objects::block::AccountWitness;
use miden_objects::crypto::SequentialCommit;
use miden_objects::crypto::merkle::InnerNodeInfo;
use miden_objects::transaction::{AccountInputs, InputNote, PartialBlockchain, TransactionInputs};
use miden_objects::vm::AdviceInputs;
use miden_objects::{EMPTY_WORD, Felt, FieldElement, Word, ZERO};
use miden_processor::AdviceMutation;
use thiserror::Error;

use super::TransactionKernel;

// TRANSACTION ADVICE INPUTS
// ================================================================================================

/// Advice inputs wrapper for inputs that are meant to be used exclusively in the transaction
/// kernel.
#[derive(Debug, Clone, Default)]
pub struct TransactionAdviceInputs(AdviceInputs);

impl TransactionAdviceInputs {
    /// Creates a [`TransactionAdviceInputs`].
    ///
    /// The created advice inputs will be populated with the data required for executing a
    /// transaction with the specified transaction inputs.
    pub fn new(tx_inputs: &TransactionInputs) -> Result<Self, TransactionAdviceMapMismatch> {
        let mut inputs = TransactionAdviceInputs(tx_inputs.advice_inputs().clone());

        inputs.build_stack(tx_inputs);
        inputs.add_kernel_commitment();
        inputs.add_partial_blockchain(tx_inputs.blockchain());
        inputs.add_input_notes(tx_inputs)?;

        // Add the script's MAST forest's advice inputs.
        if let Some(tx_script) = tx_inputs.tx_args().tx_script() {
            inputs.extend_map(
                tx_script
                    .mast()
                    .advice_map()
                    .iter()
                    .map(|(key, values)| (*key, values.to_vec())),
            );
        }

        // Inject native account.
        let partial_native_acc = tx_inputs.account();
        inputs.add_account(partial_native_acc)?;

        // If a seed was provided, extend the map appropriately.
        if let Some(seed) = tx_inputs.account().seed() {
            // ACCOUNT_ID |-> ACCOUNT_SEED
            let account_id_key = Self::account_id_map_key(partial_native_acc.id());
            inputs.add_map_entry(account_id_key, seed.to_vec());
        }

        // Inject foreign account inputs.
        inputs.add_foreign_accounts(tx_inputs.tx_args().foreign_account_inputs())?;

        // Extend with extra user-supplied advice.
        inputs.extend(tx_inputs.tx_args().advice_inputs().clone());

        Ok(inputs)
    }

    /// Returns a reference to the underlying advice inputs.
    pub fn as_advice_inputs(&self) -> &AdviceInputs {
        &self.0
    }

    /// Converts these transaction advice inputs into the underlying advice inputs.
    pub fn into_advice_inputs(self) -> AdviceInputs {
        self.0
    }

    /// Consumes self and returns an iterator of [`AdviceMutation`]s in arbitrary order.
    pub fn into_advice_mutations(self) -> impl Iterator<Item = AdviceMutation> {
        [
            AdviceMutation::ExtendMap { other: self.0.map },
            AdviceMutation::ExtendMerkleStore {
                infos: self.0.store.inner_nodes().collect(),
            },
            AdviceMutation::ExtendStack { values: self.0.stack },
        ]
        .into_iter()
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Extends these advice inputs with the provided advice inputs.
    pub fn extend(&mut self, adv_inputs: AdviceInputs) {
        self.0.extend(adv_inputs);
    }

    /// Adds the provided account inputs into the advice inputs.
    pub fn add_foreign_accounts<'inputs>(
        &mut self,
        foreign_account_inputs: impl IntoIterator<Item = &'inputs AccountInputs>,
    ) -> Result<(), TransactionAdviceMapMismatch> {
        for foreign_acc in foreign_account_inputs {
            self.add_account(foreign_acc.account())?;
            self.add_account_witness(foreign_acc.witness());

            // for foreign accounts, we need to insert the id to state mapping
            // NOTE: keep this in sync with the account::load_from_advice procedure
            let account_id_key = Self::account_id_map_key(foreign_acc.id());
            let header = AccountHeader::from(foreign_acc.account());

            // ACCOUNT_ID |-> [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT, CODE_COMMITMENT]
            self.add_map_entry(account_id_key, header.as_elements());
        }

        Ok(())
    }

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
    ///     [native_asset_id_suffix, native_asset_id_prefix, verification_base_fee, 0]
    ///     [0, 0, 0, 0]
    ///     NOTE_ROOT,
    ///     kernel_version
    ///     [account_id, 0, 0, account_nonce],
    ///     ACCOUNT_VAULT_ROOT,
    ///     ACCOUNT_STORAGE_COMMITMENT,
    ///     ACCOUNT_CODE_COMMITMENT,
    ///     number_of_input_notes,
    ///     TX_SCRIPT_ROOT,
    ///     TX_SCRIPT_ARGS,
    ///     AUTH_ARGS,
    /// ]
    fn build_stack(&mut self, tx_inputs: &TransactionInputs) {
        let header = tx_inputs.block_header();

        // --- block header data (keep in sync with kernel's process_block_data) --
        self.extend_stack(header.prev_block_commitment());
        self.extend_stack(header.chain_commitment());
        self.extend_stack(header.account_root());
        self.extend_stack(header.nullifier_root());
        self.extend_stack(header.tx_commitment());
        self.extend_stack(header.tx_kernel_commitment());
        self.extend_stack(header.proof_commitment());
        self.extend_stack([
            header.block_num().into(),
            header.version().into(),
            header.timestamp().into(),
            ZERO,
        ]);
        self.extend_stack([
            header.fee_parameters().native_asset_id().suffix(),
            header.fee_parameters().native_asset_id().prefix().as_felt(),
            header.fee_parameters().verification_base_fee().into(),
            ZERO,
        ]);
        self.extend_stack([ZERO, ZERO, ZERO, ZERO]);
        self.extend_stack(header.note_root());

        // --- core account items (keep in sync with process_account_data) ----
        let account = tx_inputs.account();
        self.extend_stack([
            account.id().suffix(),
            account.id().prefix().as_felt(),
            ZERO,
            account.nonce(),
        ]);
        self.extend_stack(account.vault().root());
        self.extend_stack(account.storage().commitment());
        self.extend_stack(account.code().commitment());

        // --- number of notes, script root and args --------------------------
        self.extend_stack([Felt::from(tx_inputs.input_notes().num_notes())]);
        let tx_args = tx_inputs.tx_args();
        self.extend_stack(tx_args.tx_script().map_or(Word::empty(), |script| script.root()));
        self.extend_stack(tx_args.tx_script_args());

        // --- auth procedure args --------------------------------------------
        self.extend_stack(tx_args.auth_args());
    }

    // BLOCKCHAIN INJECTIONS
    // --------------------------------------------------------------------------------------------

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
    fn add_partial_blockchain(&mut self, mmr: &PartialBlockchain) {
        // NOTE: keep this code in sync with the `process_chain_data` kernel procedure
        // add authentication paths from the MMR to the Merkle store
        self.extend_merkle_store(mmr.inner_nodes());

        // insert MMR peaks info into the advice map
        let peaks = mmr.peaks();
        let mut elements = vec![Felt::new(peaks.num_leaves() as u64), ZERO, ZERO, ZERO];
        elements.extend(peaks.flatten_and_pad_peaks());
        self.add_map_entry(peaks.hash_peaks(), elements);
    }

    // KERNEL INJECTIONS
    // --------------------------------------------------------------------------------------------

    /// Inserts the kernel commitment and its procedure roots into the advice map.
    ///
    /// Inserts the following entries into the advice map:
    /// - The commitment of the kernel |-> array of the kernel's procedure roots.
    fn add_kernel_commitment(&mut self) {
        // insert the kernel commitment with its procedure roots into the advice map
        self.add_map_entry(TransactionKernel.to_commitment(), TransactionKernel.to_elements());
    }

    // ACCOUNT INJECTION
    // --------------------------------------------------------------------------------------------

    /// Inserts account data into the advice inputs.
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
    fn add_account(
        &mut self,
        account: &PartialAccount,
    ) -> Result<(), TransactionAdviceMapMismatch> {
        // --- account code -------------------------------------------------------

        // CODE_COMMITMENT -> [[ACCOUNT_PROCEDURE_DATA]]
        let code = account.code();
        self.add_map_entry(code.commitment(), code.as_elements());

        // Extend the advice map with the account code's advice inputs.
        // This ensures that the advice map is available during the note script execution when it
        // calls the account's code that relies on the it's advice map data (data segments) loaded
        // into the advice provider
        self.0.map.merge(account.code().mast().advice_map()).map_err(
            |((key, existing_val), incoming_val)| TransactionAdviceMapMismatch {
                key,
                existing_val: existing_val.to_vec(),
                incoming_val: incoming_val.to_vec(),
            },
        )?;

        // --- account storage ----------------------------------------------------

        // STORAGE_COMMITMENT |-> [[STORAGE_SLOT_DATA]]
        let storage_header = account.storage().header();
        self.add_map_entry(storage_header.to_commitment(), storage_header.to_elements());

        // populate Merkle store and advice map with nodes info needed to access storage map entries
        self.extend_merkle_store(account.storage().inner_nodes());
        self.extend_map(account.storage().leaves().map(|leaf| (leaf.hash(), leaf.to_elements())));

        // --- account vault ------------------------------------------------------

        // populate Merkle store and advice map with nodes info needed to access vault assets
        self.extend_merkle_store(account.vault().inner_nodes());
        self.extend_map(account.vault().leaves().map(|leaf| (leaf.hash(), leaf.to_elements())));

        Ok(())
    }

    /// Adds an account witness to the advice inputs.
    ///
    /// This involves extending the map to include the leaf's hash mapped to its elements, as well
    /// as extending the merkle store with the nodes of the witness.
    fn add_account_witness(&mut self, witness: &AccountWitness) {
        // populate advice map with the account's leaf
        let leaf = witness.leaf();
        self.add_map_entry(leaf.hash(), leaf.to_elements());

        // extend the merkle store and map with account witnesses merkle path
        self.extend_merkle_store(witness.authenticated_nodes());
    }

    // NOTE INJECTION
    // --------------------------------------------------------------------------------------------

    /// Populates the advice inputs for all input notes.
    ///
    /// The advice provider is populated with:
    ///
    /// - For each note:
    ///     - The note's details (serial number, script root, and its input / assets commitment).
    ///     - The note's private arguments.
    ///     - The note's public metadata.
    ///     - The note's public inputs data. Prefixed by its length and padded to an even word
    ///       length.
    ///     - The note's asset padded. Prefixed by its length and padded to an even word length.
    ///     - The note's script MAST forest's advice map inputs
    ///     - For authenticated notes (determined by the `is_authenticated` flag):
    ///         - The note's authentication path against its block's note tree.
    ///         - The block number, sub commitment, note root.
    ///         - The note's position in the note tree
    ///
    /// The data above is processed by `prologue::process_input_notes_data`.
    fn add_input_notes(
        &mut self,
        tx_inputs: &TransactionInputs,
    ) -> Result<(), TransactionAdviceMapMismatch> {
        if tx_inputs.input_notes().is_empty() {
            return Ok(());
        }

        let mut note_data = Vec::new();
        for input_note in tx_inputs.input_notes().iter() {
            let note = input_note.note();
            let assets = note.assets();
            let recipient = note.recipient();
            let note_arg = tx_inputs.tx_args().get_note_args(note.id()).unwrap_or(&EMPTY_WORD);

            // recipient inputs / assets commitments
            self.add_map_entry(
                recipient.inputs().commitment(),
                recipient.inputs().format_for_advice(),
            );
            self.add_map_entry(assets.commitment(), assets.to_padded_assets());

            // note details / metadata
            note_data.extend(recipient.serial_num());
            note_data.extend(*recipient.script().root());
            note_data.extend(*recipient.inputs().commitment());
            note_data.extend(*assets.commitment());
            note_data.extend(*note_arg);
            note_data.extend(Word::from(note.metadata()));
            note_data.push(recipient.inputs().num_values().into());
            note_data.push((assets.num_assets() as u32).into());
            note_data.extend(assets.to_padded_assets());

            // authentication vs unauthenticated
            match input_note {
                InputNote::Authenticated { note, proof } => {
                    // Push the `is_authenticated` flag
                    note_data.push(Felt::ONE);

                    // Merkle path
                    self.extend_merkle_store(proof.authenticated_nodes(note.commitment()));

                    let block_num = proof.location().block_num();
                    let block_header = if block_num == tx_inputs.block_header().block_num() {
                        tx_inputs.block_header()
                    } else {
                        tx_inputs
                            .blockchain()
                            .get_block(block_num)
                            .expect("block not found in partial blockchain")
                    };

                    note_data.push(block_num.into());
                    note_data.extend(block_header.sub_commitment());
                    note_data.extend(block_header.note_root());
                    note_data.push(proof.location().node_index_in_block().into());
                },
                InputNote::Unauthenticated { .. } => {
                    // push the `is_authenticated` flag
                    note_data.push(Felt::ZERO)
                },
            }

            self.0.map.merge(note.script().mast().advice_map()).map_err(
                |((key, existing_val), incoming_val)| TransactionAdviceMapMismatch {
                    key,
                    existing_val: existing_val.to_vec(),
                    incoming_val: incoming_val.to_vec(),
                },
            )?;
        }

        self.add_map_entry(tx_inputs.input_notes().commitment(), note_data);

        Ok(())
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Extends the map of values with the given argument, replacing previously inserted items.
    fn extend_map(&mut self, iter: impl IntoIterator<Item = (Word, Vec<Felt>)>) {
        self.0.map.extend(iter);
    }

    fn add_map_entry(&mut self, key: Word, values: Vec<Felt>) {
        self.0.map.extend([(key, values)]);
    }

    /// Extends the stack with the given elements.
    fn extend_stack(&mut self, iter: impl IntoIterator<Item = Felt>) {
        self.0.stack.extend(iter);
    }

    /// Extends the [`MerkleStore`](miden_objects::crypto::merkle::MerkleStore) with the given
    /// nodes.
    fn extend_merkle_store(&mut self, iter: impl Iterator<Item = InnerNodeInfo>) {
        self.0.store.extend(iter);
    }

    /// Returns the advice map key where:
    /// - the seed for native accounts is stored.
    /// - the account header for foreign accounts is stored.
    fn account_id_map_key(id: AccountId) -> Word {
        Word::from([id.suffix(), id.prefix().as_felt(), ZERO, ZERO])
    }
}

// CONVERSIONS
// ================================================================================================

impl From<TransactionAdviceInputs> for AdviceInputs {
    fn from(wrapper: TransactionAdviceInputs) -> Self {
        wrapper.0
    }
}

impl From<AdviceInputs> for TransactionAdviceInputs {
    fn from(inner: AdviceInputs) -> Self {
        Self(inner)
    }
}

// CONFLICT ERROR
// ================================================================================================

#[derive(Debug, Error)]
#[error(
    "conflicting map entry for key {key}: existing={existing_val:?}, incoming={incoming_val:?}"
)]
pub struct TransactionAdviceMapMismatch {
    pub key: Word,
    pub existing_val: Vec<Felt>,
    pub incoming_val: Vec<Felt>,
}
