use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt::Debug;
use std::collections::BTreeSet;

use miden_core::utils::{Deserializable, Serializable};
use miden_crypto::merkle::NodeIndex;
use miden_crypto::merkle::smt::{LeafIndex, SmtLeaf, SmtProof};

use super::PartialBlockchain;
use crate::account::{
    AccountCode,
    AccountHeader,
    AccountId,
    AccountStorageHeader,
    PartialAccount,
    PartialStorage,
    StorageSlotId,
    StorageSlotName,
};
use crate::asset::{AssetVaultKey, AssetWitness, PartialVault};
use crate::block::account_tree::{AccountWitness, account_id_to_smt_index};
use crate::block::{BlockHeader, BlockNumber};
use crate::crypto::merkle::SparseMerklePath;
use crate::note::{Note, NoteInclusionProof};
use crate::transaction::{TransactionAdviceInputs, TransactionArgs, TransactionScript};
use crate::{Felt, TransactionInputError, Word};

#[cfg(test)]
mod tests;

mod account;
pub use account::AccountInputs;

mod notes;
use miden_processor::{AdviceInputs, SMT_DEPTH};
pub use notes::{InputNote, InputNotes, ToInputNoteCommitments};

// TRANSACTION INPUTS
// ================================================================================================

/// Contains the data required to execute a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionInputs {
    account: PartialAccount,
    block_header: BlockHeader,
    blockchain: PartialBlockchain,
    input_notes: InputNotes<InputNote>,
    tx_args: TransactionArgs,
    advice_inputs: AdviceInputs,
    foreign_account_code: Vec<AccountCode>,
    /// Pre-fetched asset witnesses for note assets and the fee asset.
    asset_witnesses: Vec<AssetWitness>,
    /// Storage slot names for foreign accounts.
    foreign_account_slot_names: BTreeMap<AccountId, BTreeMap<StorageSlotId, StorageSlotName>>,
}

impl TransactionInputs {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns new [`TransactionInputs`] instantiated with the specified parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The partial blockchain does not track the block headers required to prove inclusion of any
    ///   authenticated input note.
    pub fn new(
        account: PartialAccount,
        block_header: BlockHeader,
        blockchain: PartialBlockchain,
        input_notes: InputNotes<InputNote>,
    ) -> Result<Self, TransactionInputError> {
        // Check that the partial blockchain and block header are consistent.
        if blockchain.chain_length() != block_header.block_num() {
            return Err(TransactionInputError::InconsistentChainLength {
                expected: block_header.block_num(),
                actual: blockchain.chain_length(),
            });
        }
        if blockchain.peaks().hash_peaks() != block_header.chain_commitment() {
            return Err(TransactionInputError::InconsistentChainCommitment {
                expected: block_header.chain_commitment(),
                actual: blockchain.peaks().hash_peaks(),
            });
        }
        // Validate the authentication paths of the input notes.
        for note in input_notes.iter() {
            if let InputNote::Authenticated { note, proof } = note {
                let note_block_num = proof.location().block_num();
                let block_header = if note_block_num == block_header.block_num() {
                    &block_header
                } else {
                    blockchain.get_block(note_block_num).ok_or(
                        TransactionInputError::InputNoteBlockNotInPartialBlockchain(note.id()),
                    )?
                };
                validate_is_in_block(note, proof, block_header)?;
            }
        }

        Ok(Self {
            account,
            block_header,
            blockchain,
            input_notes,
            tx_args: TransactionArgs::default(),
            advice_inputs: AdviceInputs::default(),
            foreign_account_code: Vec::new(),
            asset_witnesses: Vec::new(),
            foreign_account_slot_names: BTreeMap::new(),
        })
    }

    /// Replaces the transaction inputs and assigns the given asset witnesses.
    pub fn with_asset_witnesses(mut self, witnesses: Vec<AssetWitness>) -> Self {
        self.asset_witnesses = witnesses;
        self
    }

    /// Replaces the transaction inputs and assigns the given foreign account code.
    pub fn with_foreign_account_code(mut self, foreign_account_code: Vec<AccountCode>) -> Self {
        self.foreign_account_code = foreign_account_code;
        self
    }

    /// Replaces the transaction inputs and assigns the given transaction arguments.
    pub fn with_tx_args(mut self, tx_args: TransactionArgs) -> Self {
        self.set_tx_args_inner(tx_args);
        self
    }

    /// Replaces the transaction inputs and assigns the given foreign account slot names.
    pub fn with_foreign_account_slot_names(
        mut self,
        foreign_account_slot_names: BTreeMap<AccountId, BTreeMap<StorageSlotId, StorageSlotName>>,
    ) -> Self {
        self.foreign_account_slot_names = foreign_account_slot_names;
        self
    }

    /// Replaces the transaction inputs and assigns the given advice inputs.
    pub fn with_advice_inputs(mut self, advice_inputs: AdviceInputs) -> Self {
        self.set_advice_inputs(advice_inputs);
        self
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Replaces the input notes for the transaction.
    pub fn set_input_notes(&mut self, new_notes: Vec<Note>) {
        self.input_notes = new_notes.into();
    }

    /// Replaces the advice inputs for the transaction.
    ///
    /// Note: the advice stack from the provided advice inputs is discarded.
    pub fn set_advice_inputs(&mut self, new_advice_inputs: AdviceInputs) {
        let AdviceInputs { map, store, .. } = new_advice_inputs;
        self.advice_inputs = AdviceInputs { stack: Default::default(), map, store };
        self.tx_args.extend_advice_inputs(self.advice_inputs.clone());
    }

    /// Updates the transaction arguments of the inputs.
    #[cfg(feature = "testing")]
    pub fn set_tx_args(&mut self, tx_args: TransactionArgs) {
        self.set_tx_args_inner(tx_args);
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the account against which the transaction is executed.
    pub fn account(&self) -> &PartialAccount {
        &self.account
    }

    /// Returns block header for the block referenced by the transaction.
    pub fn block_header(&self) -> &BlockHeader {
        &self.block_header
    }

    /// Returns partial blockchain containing authentication paths for all notes consumed by the
    /// transaction.
    pub fn blockchain(&self) -> &PartialBlockchain {
        &self.blockchain
    }

    /// Returns the notes to be consumed in the transaction.
    pub fn input_notes(&self) -> &InputNotes<InputNote> {
        &self.input_notes
    }

    /// Returns the block number referenced by the inputs.
    pub fn ref_block(&self) -> BlockNumber {
        self.block_header.block_num()
    }

    /// Returns the transaction script to be executed.
    pub fn tx_script(&self) -> Option<&TransactionScript> {
        self.tx_args.tx_script()
    }

    /// Returns the foreign account code to be executed.
    pub fn foreign_account_code(&self) -> &[AccountCode] {
        &self.foreign_account_code
    }

    /// Returns the pre-fetched witnesses for note and fee assets.
    pub fn asset_witnesses(&self) -> &[AssetWitness] {
        &self.asset_witnesses
    }

    /// Returns the foreign account storage slot names.
    pub fn foreign_account_slot_names(
        &self,
    ) -> &BTreeMap<AccountId, BTreeMap<StorageSlotId, StorageSlotName>> {
        &self.foreign_account_slot_names
    }

    /// Returns the advice inputs to be consumed in the transaction.
    pub fn advice_inputs(&self) -> &AdviceInputs {
        &self.advice_inputs
    }

    /// Returns the transaction arguments to be consumed in the transaction.
    pub fn tx_args(&self) -> &TransactionArgs {
        &self.tx_args
    }

    /// Reads the vault asset witnesses for the given account and vault keys.
    pub fn read_vault_asset_witnesses(
        &self,
        account_id: AccountId,
        vault_keys: BTreeSet<AssetVaultKey>,
    ) -> Result<Vec<AssetWitness>, TransactionInputError> {
        // Read the account header elements from the advice map.
        let account_id_key = TransactionAdviceInputs::account_id_map_key(account_id);
        let header_elements = self
            .advice_inputs
            .map
            .get(&account_id_key)
            .ok_or(TransactionInputError::ForeignAccountNotFound(account_id))?;

        let header = AccountHeader::try_from_elements(header_elements)?;
        let vault_root = header.vault_root();

        let mut asset_witnesses = Vec::new();
        for vault_key in vault_keys {
            let smt_index = vault_key.to_leaf_index();
            // Construct sparse Merkle path.
            let merkle_path = self.advice_inputs.store.get_path(vault_root, smt_index.into())?;
            let sparse_path = SparseMerklePath::from_sized_iter(merkle_path.path)?;

            // Construct SMT leaf.
            let merkle_node = self.advice_inputs.store.get_node(vault_root, smt_index.into())?;
            let smt_leaf_elements = self
                .advice_inputs
                .map
                .get(&merkle_node)
                .ok_or(TransactionInputError::MissingVaultRoot)?;
            let smt_leaf = smt_leaf_from_elements(smt_leaf_elements, smt_index);

            // Construct SMT proof and witness.
            let smt_proof = SmtProof::new(sparse_path, smt_leaf)?;
            let asset_witness = AssetWitness::new(smt_proof)?;
            asset_witnesses.push(asset_witness);
        }
        Ok(asset_witnesses)
    }

    /// Reads AccountInputs for a foreign account from the advice inputs.
    ///
    /// This function reverses the process of [`TransactionAdviceInputs::add_foreign_accounts`] by:
    /// 1. Reading the account header from the advice map using the account_id_key.
    /// 2. Building a PartialAccount from the header and foreign account code.
    /// 3. Creating an AccountWitness.
    pub fn read_foreign_account_inputs(
        &self,
        account_id: AccountId,
    ) -> Result<AccountInputs, TransactionInputError> {
        if account_id == self.account().id() {
            return Err(TransactionInputError::AccountNotForeign);
        }

        // Read the account header elements from the advice map.
        let account_id_key = TransactionAdviceInputs::account_id_map_key(account_id);
        let header_elements = self
            .advice_inputs
            .map
            .get(&account_id_key)
            .ok_or(TransactionInputError::ForeignAccountNotFound(account_id))?;

        // Parse the header from elements.
        let header = AccountHeader::try_from_elements(header_elements)?;

        // Construct and return account inputs.
        let partial_account = self.read_foreign_partial_account(&header)?;
        let witness = self.read_foreign_account_witness(&header)?;
        Ok(AccountInputs::new(partial_account, witness))
    }

    /// Reads a foreign partial account from the advice inputs based on the account ID corresponding
    /// to the provided header.
    fn read_foreign_partial_account(
        &self,
        header: &AccountHeader,
    ) -> Result<PartialAccount, TransactionInputError> {
        // Derive the partial vault from the header.
        let partial_vault = PartialVault::new(header.vault_root());

        // Find the corresponding foreign account code.
        let account_code = self
            .foreign_account_code
            .iter()
            .find(|code| code.commitment() == header.code_commitment())
            .ok_or(TransactionInputError::ForeignAccountCodeNotFound(header.id()))?
            .clone();

        // Try to get storage header from advice map using storage commitment as key.
        let storage_header_elements = self
            .advice_inputs
            .map
            .get(&header.storage_commitment())
            .ok_or(TransactionInputError::StorageHeaderNotFound(header.id()))?;

        // Get slot names for this foreign account, or use empty map if not available.
        let empty_slot_names = BTreeMap::new();
        let slot_names =
            self.foreign_account_slot_names.get(&header.id()).unwrap_or(&empty_slot_names);

        let storage_header =
            AccountStorageHeader::try_from_elements(storage_header_elements, slot_names)?;

        // Build partial storage.
        let partial_storage = PartialStorage::new(storage_header, [])?;

        // Create the partial account.
        let partial_account = PartialAccount::new(
            header.id(),
            header.nonce(),
            account_code,
            partial_storage,
            partial_vault,
            None, // We know that foreign accounts are existing accounts so a seed is not required.
        )?;

        Ok(partial_account)
    }

    /// Reads a foreign account witness from the advice inputs based on the account ID corresponding
    /// to the provided header.
    fn read_foreign_account_witness(
        &self,
        header: &AccountHeader,
    ) -> Result<AccountWitness, TransactionInputError> {
        // Get the account tree root from the block header.
        let account_tree_root = self.block_header.account_root();
        let leaf_index = account_id_to_smt_index(header.id());
        let smt_index = NodeIndex::new(64, leaf_index.value())?;

        // Get the Merkle path from the merkle store.
        let merkle_path = self.advice_inputs.store.get_path(account_tree_root, smt_index)?;

        // Convert the Merkle path to SparseMerklePath.
        let sparse_path = SparseMerklePath::from_sized_iter(merkle_path.path)?;

        // Create the account witness.
        let witness = AccountWitness::new(header.id(), header.commitment(), sparse_path)?;

        Ok(witness)
    }

    // CONVERSIONS
    // --------------------------------------------------------------------------------------------

    /// Consumes these transaction inputs and returns their underlying components.
    pub fn into_parts(
        self,
    ) -> (
        PartialAccount,
        BlockHeader,
        PartialBlockchain,
        InputNotes<InputNote>,
        TransactionArgs,
    ) {
        (self.account, self.block_header, self.blockchain, self.input_notes, self.tx_args)
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Replaces the current tx_args with the provided value.
    ///
    /// This also appends advice inputs from these transaction inputs to the advice inputs of the
    /// tx args.
    fn set_tx_args_inner(&mut self, tx_args: TransactionArgs) {
        self.tx_args = tx_args;
        self.tx_args.extend_advice_inputs(self.advice_inputs.clone());
    }
}

impl Serializable for TransactionInputs {
    fn write_into<W: miden_core::utils::ByteWriter>(&self, target: &mut W) {
        self.account.write_into(target);
        self.block_header.write_into(target);
        self.blockchain.write_into(target);
        self.input_notes.write_into(target);
        self.tx_args.write_into(target);
        self.advice_inputs.write_into(target);
        self.foreign_account_code.write_into(target);
        self.asset_witnesses.write_into(target);

        // Serialize foreign account slot names.
        target.write_u32(self.foreign_account_slot_names.len() as u32);
        for (account_id, slot_names) in &self.foreign_account_slot_names {
            account_id.write_into(target);
            target.write_u32(slot_names.len() as u32);
            for (slot_id, slot_name) in slot_names {
                slot_id.write_into(target);
                slot_name.write_into(target);
            }
        }
    }
}

impl Deserializable for TransactionInputs {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, miden_core::utils::DeserializationError> {
        let account = PartialAccount::read_from(source)?;
        let block_header = BlockHeader::read_from(source)?;
        let blockchain = PartialBlockchain::read_from(source)?;
        let input_notes = InputNotes::read_from(source)?;
        let tx_args = TransactionArgs::read_from(source)?;
        let advice_inputs = AdviceInputs::read_from(source)?;
        let foreign_account_code = Vec::<AccountCode>::read_from(source)?;
        let asset_witnesses = Vec::<AssetWitness>::read_from(source)?;

        // Deserialize foreign account slot names.
        let num_accounts = source.read_u32()? as usize;
        let mut foreign_account_slot_names = BTreeMap::new();
        for _ in 0..num_accounts {
            let account_id = AccountId::read_from(source)?;
            let num_slots = source.read_u32()? as usize;
            let mut slot_names = BTreeMap::new();
            for _ in 0..num_slots {
                let slot_id = StorageSlotId::read_from(source)?;
                let slot_name = StorageSlotName::read_from(source)?;
                slot_names.insert(slot_id, slot_name);
            }
            foreign_account_slot_names.insert(account_id, slot_names);
        }

        Ok(TransactionInputs {
            account,
            block_header,
            blockchain,
            input_notes,
            tx_args,
            advice_inputs,
            foreign_account_code,
            asset_witnesses,
            foreign_account_slot_names,
        })
    }
}

// HELPER FUNCTIONS
// ================================================================================================

// TODO(sergerad): Move this fn to crypto SmtLeaf::try_from_elements. Panics will be replaced with
// SmtLeafError variants when moved.
pub fn smt_leaf_from_elements(elements: &[Felt], leaf_index: LeafIndex<SMT_DEPTH>) -> SmtLeaf {
    use miden_crypto::merkle::smt::SmtLeaf;

    // Based on the miden-crypto SMT leaf serialization format.

    if elements.is_empty() {
        return SmtLeaf::new_empty(leaf_index);
    }

    // Elements should be organized into a contiguous array of K/V Words (4 Felts each).
    if !elements.len().is_multiple_of(8) {
        panic!("invalid SMT leaf format: elements length must be divisible by 8");
    }

    let num_entries = elements.len() / 8;

    if num_entries == 1 {
        // Single entry.
        let key = Word::new([elements[0], elements[1], elements[2], elements[3]]);
        let value = Word::new([elements[4], elements[5], elements[6], elements[7]]);
        SmtLeaf::new_single(key, value)
    } else {
        // Multiple entries.
        let mut entries = Vec::with_capacity(num_entries);
        // Read k/v pairs from each entry.
        for i in 0..num_entries {
            let base_idx = i * 8;
            let key = Word::new([
                elements[base_idx],
                elements[base_idx + 1],
                elements[base_idx + 2],
                elements[base_idx + 3],
            ]);
            let value = Word::new([
                elements[base_idx + 4],
                elements[base_idx + 5],
                elements[base_idx + 6],
                elements[base_idx + 7],
            ]);
            entries.push((key, value));
        }
        SmtLeaf::new_multiple(entries).expect("failed to create multiple leaf")
    }
}

/// Validates whether the provided note belongs to the note tree of the specified block.
fn validate_is_in_block(
    note: &Note,
    proof: &NoteInclusionProof,
    block_header: &BlockHeader,
) -> Result<(), TransactionInputError> {
    let note_index = proof.location().node_index_in_block().into();
    let note_commitment = note.commitment();
    proof
        .note_path()
        .verify(note_index, note_commitment, &block_header.note_root())
        .map_err(|_| {
            TransactionInputError::InputNoteNotInBlock(note.id(), proof.location().block_num())
        })
}
