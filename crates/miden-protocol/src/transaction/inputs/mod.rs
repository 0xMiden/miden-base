use alloc::vec::Vec;
use core::fmt::Debug;

use miden_core::utils::{Deserializable, Serializable};

use super::PartialBlockchain;
use crate::TransactionInputError;
use crate::account::{
    AccountCode,
    AccountHeader,
    AccountId,
    AccountStorageHeader,
    PartialAccount,
    PartialStorage,
};
use crate::asset::{AssetWitness, PartialVault};
use crate::block::account_tree::{AccountWitness, account_id_to_smt_index};
use crate::block::{BlockHeader, BlockNumber};
use crate::crypto::merkle::SparseMerklePath;
use crate::note::{Note, NoteInclusionProof};
use crate::transaction::{TransactionAdviceInputs, TransactionArgs, TransactionScript};

mod account;
pub use account::AccountInputs;

mod notes;
use miden_processor::AdviceInputs;
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

    /// Returns the advice inputs to be consumed in the transaction.
    pub fn advice_inputs(&self) -> &AdviceInputs {
        &self.advice_inputs
    }

    /// Returns the transaction arguments to be consumed in the transaction.
    pub fn tx_args(&self) -> &TransactionArgs {
        &self.tx_args
    }

    /// Reads AccountInputs for a foreign account from the advice inputs.
    ///
    /// This function reverses the process of `add_foreign_accounts` by:
    /// 1. Reading the account header from the advice map using the account_id_key
    /// 2. Building a PartialAccount from the header and foreign account code
    /// 3. Creating an AccountWitness (currently with placeholder path)
    ///
    /// The account header is stored in the advice map under a key derived from the account ID,
    /// containing [ID_AND_NONCE, VAULT_ROOT, STORAGE_COMMITMENT, CODE_COMMITMENT] elements.
    /// The corresponding foreign account code must be present in the foreign_account_code list.
    ///
    /// # Implementation Notes
    ///
    /// Currently, the AccountWitness is created with a placeholder Merkle path. For a complete
    /// implementation, the witness should be reconstructed from the Merkle store data that was
    /// added via `add_account_witness`. This would require:
    ///
    /// 1. Implementing `MerkleStore::get_path(root, index)` method or equivalent
    /// 2. Using `account_id_to_smt_index()` to convert the account ID to the correct index
    /// 3. Retrieving the authenticated nodes from the merkle store
    /// 4. Reconstructing the SparseMerklePath from the stored InnerNodeInfo data
    ///
    /// The authenticated nodes are stored in the merkle store via `witness.authenticated_nodes()`
    /// in the `add_account_witness` function.
    ///
    /// # Arguments
    /// * `account_id` - The ID of the foreign account to read
    ///
    /// # Returns
    /// * `Ok(AccountInputs)` if the account data can be successfully read and parsed
    /// * `Err(TransactionInputError)` if any required data is missing or invalid
    ///
    /// # Errors
    ///
    /// This function will return `TransactionInputError::ForeignAccountError` if:
    /// - The account header is not found in the advice map
    /// - The account header cannot be parsed from the stored elements
    /// - The foreign account code is not found in the foreign_account_code list
    /// - Any component of the PartialAccount cannot be constructed
    /// - The AccountWitness cannot be created
    pub fn read_foreign_account_inputs(
        &self,
        account_id: AccountId,
    ) -> Result<AccountInputs, TransactionInputError> {
        if account_id == self.account().id() {
            return Err(TransactionInputError::AccountNotForeign);
        }

        // Create the account_id_key.
        let account_id_key = TransactionAdviceInputs::account_id_map_key(account_id);

        // Read the account header elements from the advice map.
        let header_elements = self
            .advice_inputs
            .map
            .get(&account_id_key)
            .ok_or(TransactionInputError::ForeignAccountNotFound(account_id))?;

        // Parse the header from elements.
        let header = AccountHeader::try_from_elements(header_elements)?;

        // Find the corresponding foreign account code
        let account_code = self
            .foreign_account_code
            .iter()
            .find(|code| code.commitment() == header.code_commitment())
            .ok_or(TransactionInputError::ForeignAccountCodeNotFound(account_id))?
            .clone();

        // Build partial account components
        // Note: We create minimal partial storage and vault since foreign accounts
        // typically don't need full state access
        let empty_header = AccountStorageHeader::new(vec![])
            .map_err(|_| TransactionInputError::ForeignAccountCodeNotFound(account_id))?;
        let partial_storage = PartialStorage::new(empty_header, [])
            .map_err(|_| TransactionInputError::ForeignAccountCodeNotFound(account_id))?;
        let partial_vault = PartialVault::new(header.vault_root());

        // Create the partial account
        let partial_account = PartialAccount::new(
            account_id,
            header.nonce(),
            account_code,
            partial_storage,
            partial_vault,
            None, // No seed for existing accounts.
        )?;

        // Create the account witness
        //
        // IMPORTANT: This is currently a placeholder implementation. For a complete solution,
        // the Merkle path should be reconstructed from the advice store data.
        //
        // The complete implementation would:
        // 1. Get the account tree root from the block header
        // 2. Convert account_id to SMT index using account_id_to_smt_index()
        // 3. Retrieve the Merkle path from the store using something like: let merkle_path =
        //    self.advice_inputs.store.get_path(account_tree_root, smt_index)?;
        // 4. Convert to SparseMerklePath and create the witness
        //
        // For now, we create a minimal witness that would need to be replaced
        // with the actual stored witness data.
        let account_tree_root = self.block_header.account_root();
        let _smt_index = account_id_to_smt_index(account_id);

        // Create a placeholder empty path - this should be replaced with actual path reconstruction
        // The authenticated nodes were stored via witness.authenticated_nodes() in
        // add_account_witness
        let empty_nodes = vec![account_tree_root; 64];
        let sparse_path = SparseMerklePath::from_sized_iter(empty_nodes)
            .map_err(|_| TransactionInputError::ForeignAccountCodeNotFound(account_id))?;

        let witness = AccountWitness::new(account_id, header.commitment(), sparse_path)
            .map_err(|_| TransactionInputError::ForeignAccountCodeNotFound(account_id))?;

        // Create and return the AccountInputs
        Ok(AccountInputs::new(partial_account, witness))
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

        Ok(TransactionInputs {
            account,
            block_header,
            blockchain,
            input_notes,
            tx_args,
            advice_inputs,
            foreign_account_code,
            asset_witnesses,
        })
    }
}

// HELPER FUNCTIONS
// ================================================================================================

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

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::account::{AccountCode, AccountStorageHeader, PartialStorage};
    use crate::asset::PartialVault;
    use crate::testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE;
    use crate::{Felt, Word};

    #[test]
    fn test_read_foreign_account_inputs_missing_data() {
        let account_id =
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap();

        // Create minimal transaction inputs with empty advice map
        let code = AccountCode::mock();
        let storage_header = AccountStorageHeader::new(vec![]).unwrap();
        let partial_storage = PartialStorage::new(storage_header, []).unwrap();
        let partial_vault = PartialVault::new(Word::default());
        let partial_account = PartialAccount::new(
            account_id,
            Felt::new(10),
            code,
            partial_storage,
            partial_vault,
            None,
        )
        .unwrap();

        let tx_inputs = TransactionInputs {
            account: partial_account,
            block_header: crate::block::BlockHeader::mock(0, None, None, &[], Word::default()),
            blockchain: crate::transaction::PartialBlockchain::default(),
            input_notes: crate::transaction::InputNotes::new(vec![]).unwrap(),
            tx_args: crate::transaction::TransactionArgs::default(),
            advice_inputs: crate::vm::AdviceInputs::default(),
            foreign_account_code: Vec::new(),
            asset_witnesses: Vec::new(),
        };

        // Try to read foreign account that doesn't exist in advice map
        let result = tx_inputs.read_foreign_account_inputs(account_id);

        assert!(
            matches!(result, Err(TransactionInputError::ForeignAccountCodeNotFound(id)) if id == account_id)
        );
    }
}
