use core::fmt::Debug;

use super::PartialBlockchain;
use crate::TransactionInputError;
use crate::account::PartialAccount;
use crate::block::BlockHeader;
use crate::note::{Note, NoteInclusionProof};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};

mod account;
pub use account::AccountInputs;

mod notes;
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
}

impl TransactionInputs {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns new [`TransactionInputs`] instantiated with the specified parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The partial blockchain's length is not the number of the reference block.
    /// - The partial blockchain's commitment does not match the reference block's chain commitment.
    /// - The partial blockchain does not proof inclusion of an authenticated input note.
    pub fn new(
        partial_account: impl Into<PartialAccount>,
        block_header: BlockHeader,
        blockchain: PartialBlockchain,
        input_notes: InputNotes<InputNote>,
    ) -> Result<Self, TransactionInputError> {
        // Check that the partial blockchain and block header are consistent.
        let block_num = block_header.block_num();
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
                let block_header = if note_block_num == block_num {
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
            account: partial_account.into(),
            block_header,
            blockchain,
            input_notes,
        })
    }

    /// Updates the input notes for the transaction.
    ///
    /// # Warning
    ///
    /// This method does not validate the notes against the data already in this
    /// [`TransactionInputs`]. It should only be called with notes that have been validated by
    /// the constructor.
    pub fn set_input_notes_unchecked(&mut self, new_notes: InputNotes<InputNote>) {
        self.input_notes = new_notes;
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

    // CONVERSIONS
    // --------------------------------------------------------------------------------------------

    /// Consumes these transaction inputs and returns their underlying components.
    pub fn into_parts(
        self,
    ) -> (PartialAccount, BlockHeader, PartialBlockchain, InputNotes<InputNote>) {
        (self.account, self.block_header, self.blockchain, self.input_notes)
    }
}

impl Serializable for TransactionInputs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account.write_into(target);
        self.block_header.write_into(target);
        self.blockchain.write_into(target);
        self.input_notes.write_into(target);
    }
}

impl Deserializable for TransactionInputs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let partial_account = PartialAccount::read_from(source)?;
        let block_header = BlockHeader::read_from(source)?;
        let blockchain = PartialBlockchain::read_from(source)?;
        let input_notes = InputNotes::read_from(source)?;

        Self::new(partial_account, block_header, blockchain, input_notes)
            .map_err(|err| DeserializationError::InvalidValue(format!("{err}")))
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
