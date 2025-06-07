use alloc::vec::Vec;

use miden_objects::{
    Digest, EMPTY_WORD, Felt, FieldElement, TransactionInputError, WORD_SIZE, Word, ZERO,
    account::{AccountHeader, AccountId, PartialAccount},
    block::AccountWitness,
    crypto::merkle::{InnerNodeInfo, SmtProof},
    note::{Note, NoteInclusionProof},
    transaction::{
        InputNote, PartialBlockchain, TransactionArgs, TransactionInputs, TransactionScript,
    },
    vm::AdviceInputs,
};

use super::TransactionKernel;

// TRANSACTION ADVICE INPUTS
// ================================================================================================

/// Advice inputs wrapper for inputs that are meant to be used exclusively on the transaction
/// kernel.
#[derive(Default, Clone, Debug)]
pub struct TransactionAdviceInputs(AdviceInputs);

impl TransactionAdviceInputs {
    /// Creates a [`TransactionAdviceInputs`].
    ///
    /// The created advice inputs will be populated with the data required for executing a
    /// transaction with the specified inputs and arguments. This includes the initial account, an
    /// optional account seed (required for new accounts), and the input note data, including
    /// core note data + authentication paths all the way to the root of one of partial
    /// blockchain peaks.
    pub fn new(
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
    ) -> Result<Self, TransactionInputError> {
        let mut inputs = TransactionAdviceInputs::default();
        let kernel_version = 0; // TODO: replace with user input

        inputs.build_stack(tx_inputs, tx_args.tx_script(), kernel_version);
        inputs.add_kernel_commitments(kernel_version);
        inputs.add_partial_blockchain(tx_inputs.block_chain());
        inputs.add_input_notes(tx_inputs, tx_args)?;

        // native/executor account
        let native_acc = PartialAccount::from(tx_inputs.account());
        inputs.add_account(&native_acc)?;

        // if a seed was provided, extend the map appropriately
        if let Some(seed) = tx_inputs.account_seed() {
            let account_id_key: Digest = Digest::from([
                native_acc.id().suffix(),
                native_acc.id().prefix().as_felt(),
                ZERO,
                ZERO,
            ]);

            inputs.extend_map([
                // ACCOUNT_ID -> ACCOUNT_SEED
                (account_id_key, seed.to_vec()),
            ]);
        }

        // foreign accounts
        for foreign_acc in tx_args.foreign_account_inputs() {
            inputs.add_account(foreign_acc.account())?;
            inputs.add_account_witness(foreign_acc.witness())?;

            // for foreign accounts, we need to insert the id to state mapping
            // NOTE: keep this in sync with the start_foreign_context kernel procedure
            let account_id_key: Digest = Digest::from([
                foreign_acc.id().suffix(),
                foreign_acc.id().prefix().as_felt(),
                ZERO,
                ZERO,
            ]);

            let header = AccountHeader::from(foreign_acc.account());
            inputs.extend_map([
                // ACCOUNT_ID -> [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT,
                // CODE_COMMITMENT]
                (account_id_key, header.as_elements()),
            ]);
        }

        // any extra user-supplied advice
        inputs.extend(tx_args.advice_inputs().clone());

        Ok(inputs)
    }

    pub fn into_advice_inputs(self) -> AdviceInputs {
        self.0
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    pub fn extend(&mut self, adv_inputs: AdviceInputs) {
        self.0.extend(adv_inputs);
    }

    /// Extends the map of values with the given argument, replacing previously inserted items.
    pub fn extend_map(&mut self, iter: impl IntoIterator<Item = (Digest, Vec<Felt>)>) {
        self.0.extend_map(iter);
    }

    /// Extends the stack with the given elements.
    pub fn extend_stack(&mut self, iter: impl IntoIterator<Item = Felt>) {
        self.0.stack.extend(iter);
    }

    /// Extends the [`MerkleStore`](miden_objects::crypto::merkle::MerkleStore) with the given
    /// nodes.
    pub fn extend_merkle_store(&mut self, iter: impl Iterator<Item = InnerNodeInfo>) {
        self.0.store.extend(iter);
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
    fn build_stack(
        &mut self,
        tx_inputs: &TransactionInputs,
        tx_script: Option<&TransactionScript>,
        kernel_version: u8,
    ) {
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
        self.extend_stack(header.note_root());

        // --- kernel version (keep in sync with process_kernel_data) -------------
        self.extend_stack([Felt::from(kernel_version)]);

        // --- core account items (keep in sync with process_account_data) --------
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

        // --- number of notes, script root and args key --------------------------
        self.extend_stack([Felt::from(tx_inputs.input_notes().num_notes())]);
        self.extend_stack(tx_script.map_or(Word::default(), |s| *s.root()));
        self.extend_stack(
            tx_script.map_or(Word::default(), |script| *script.args_key().unwrap_or_default()),
        );
    }

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
        self.extend_map([(peaks.hash_peaks(), elements)]);
    }

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
    fn add_account(&mut self, account: &PartialAccount) -> Result<(), TransactionInputError> {
        // --- account storage ----------------------------------------------------

        let storage_header = account.storage().header();
        self.extend_map([
            // STORAGE_COMMITMENT -> [[STORAGE_SLOT_DATA]]
            (storage_header.compute_commitment(), storage_header.as_elements()),
        ]);

        // load merkle nodes for storage maps
        for proof in account.storage().storage_map_proofs() {
            // extend the merkle store with the SMT proof
            self.extend_merkle_store_with_smt_proof(proof, account.id())?;
            // populate advice map with Sparse Merkle Tree leaf nodes
            self.extend_map(core::iter::once((proof.leaf().hash(), proof.leaf().to_elements())));
        }

        // --- account code -------------------------------------------------------

        let code = account.code();
        self.extend_map([
            // CODE_COMMITMENT -> [[ACCOUNT_PROCEDURE_DATA]]
            (code.commitment(), code.as_elements()),
        ]);

        // --- account vault ------------------------------------------------------

        self.extend_merkle_store(account.vault().inner_nodes());
        // populate advice map with Sparse Merkle Tree leaf nodes
        self.extend_map(account.vault().leaves().map(|leaf| (leaf.hash(), leaf.to_elements())));

        Ok(())
    }

    /// Adds an account witness to the advice inputs.
    ///
    /// This involves extending the map to include the leaf's hash mapped to its elements, as well
    /// as extending the merkle store with the nodes of the witness.
    fn add_account_witness(
        &mut self,
        witness: &AccountWitness,
    ) -> Result<(), TransactionInputError> {
        let leaf = witness.leaf();

        // populate advice map with the account's leaf
        self.extend_map([(leaf.hash(), leaf.to_elements())]);
        // extend the merkle store and map with account witnesses merkle path
        self.extend_merkle_store(witness.inner_nodes());
        Ok(())
    }

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
    ///     - For authenticated notes (determined by the `is_authenticated` flag):
    ///         - The note's authentication path against its block's note tree.
    ///         - The block number, sub commitment, note root.
    ///         - The note's position in the note tree
    ///
    /// The data above is processed by `prologue::process_input_notes_data`.
    fn add_input_notes(
        &mut self,
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
    ) -> Result<(), TransactionInputError> {
        if tx_inputs.input_notes().is_empty() {
            return Ok(());
        }

        let mut note_data = Vec::new();
        for input_note in tx_inputs.input_notes().iter() {
            let note = input_note.note();
            let assets = note.assets();
            let recipient = note.recipient();
            let note_arg = tx_args.get_note_args(note.id()).unwrap_or(&EMPTY_WORD);

            // recipient inputs / assets commitments
            self.extend_map([(
                recipient.inputs().commitment(),
                recipient.inputs().format_for_advice(),
            )]);
            self.extend_map([(assets.commitment(), assets.to_padded_assets())]);

            // note details / metadata
            note_data.extend(recipient.serial_num());
            note_data.extend(*recipient.script().root());
            note_data.extend(*recipient.inputs().commitment());
            note_data.extend(*assets.commitment());
            note_data.extend(Word::from(*note_arg));
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
                    self.extend_with_note_merkle_proof(proof, note)?;

                    let block_num = proof.location().block_num();
                    let block_header = if block_num == tx_inputs.block_header().block_num() {
                        tx_inputs.block_header()
                    } else {
                        tx_inputs
                            .block_chain()
                            .get_block(block_num)
                            .expect("block not found in partial blockchain")
                    };

                    note_data.push(block_num.into());
                    note_data.extend(block_header.sub_commitment());
                    note_data.extend(block_header.note_root());
                    note_data.push(proof.location().node_index_in_block().into());
                },
                InputNote::Unauthenticated { .. } =>
                // push the `is_authenticated` flag
                {
                    note_data.push(Felt::ZERO)
                },
            }
        }

        self.extend_map([(tx_inputs.input_notes().commitment(), note_data)]);
        Ok(())
    }

    /// Inserts kernel commitments and hashes of their procedures into the provided advice inputs.
    ///
    /// Inserts the following entries into the advice map:
    /// - The accumulative hash of all kernels |-> array of each kernel commitment.
    /// - The hash of the selected kernel |-> array of the kernel's procedure roots.
    fn add_kernel_commitments(&mut self, kernel_version: u8) {
        let mut kernel_commitments: Vec<Felt> =
            Vec::with_capacity(TransactionKernel::NUM_VERSIONS * WORD_SIZE);
        for version in 0..TransactionKernel::NUM_VERSIONS {
            kernel_commitments
                .extend_from_slice(TransactionKernel::commitment(version as u8).as_elements());
        }

        // insert the selected kernel commitment with its procedure roots into the advice map
        self.extend_map([(
            Digest::new(
                kernel_commitments[kernel_version as usize..kernel_version as usize + WORD_SIZE]
                    .try_into()
                    .expect("invalid kernel offset"),
            ),
            TransactionKernel::procedures_as_elements(kernel_version),
        )]);

        // insert kernels root with kernel commitments into the advice map
        self.extend_map([(TransactionKernel::kernel_commitment(), kernel_commitments)]);
    }

    /// Extends the [`MerkleStore`](miden_objects::crypto::merkle::MerkleStore) with the given
    /// inclusion proof for a note.
    fn extend_with_note_merkle_proof(
        &mut self,
        note_inclusion_proof: &NoteInclusionProof,
        note: &Note,
    ) -> Result<(), TransactionInputError> {
        match note_inclusion_proof
            .note_path()
            .inner_nodes(note_inclusion_proof.location_index().into(), note.commitment())
        {
            Ok(nodes) => {
                self.extend_merkle_store(nodes);
                Ok(())
            },
            Err(err) => Err(TransactionInputError::InvalidNoteMerklePath(note.id(), err)),
        }
    }

    /// Extends the [`MerkleStore`](miden_objects::crypto::merkle::MerkleStore) with the given
    /// SMT proof for an account.
    fn extend_merkle_store_with_smt_proof(
        &mut self,
        smt_proof: &SmtProof,
        acc_id: AccountId,
    ) -> Result<(), TransactionInputError> {
        match smt_proof
            .path()
            .inner_nodes(smt_proof.leaf().index().value(), smt_proof.leaf().hash())
        {
            Ok(nodes) => {
                self.extend_merkle_store(nodes);
                Ok(())
            },
            Err(err) => Err(TransactionInputError::InvalidAccountMerklePath(acc_id, err)),
        }
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
