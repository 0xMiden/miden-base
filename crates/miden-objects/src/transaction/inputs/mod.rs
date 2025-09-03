use core::fmt::Debug;

use super::PartialBlockchain;
use crate::account::{AccountId, PartialAccount};
use crate::block::BlockHeader;
use crate::note::{Note, NoteInclusionProof};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{TransactionInputError, Word};

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
    account_seed: Option<Word>,
    block_header: BlockHeader,
    block_chain: PartialBlockchain,
    input_notes: InputNotes<InputNote>,
}

impl TransactionInputs {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [TransactionInputs] instantiated with the specified parameters.
    ///
    /// # Errors
    /// Returns an error if:
    /// - For a new account, account seed is not provided or the provided seed is invalid.
    /// - For an existing account, account seed was provided.
    pub fn new(
        account: impl Into<PartialAccount>,
        account_seed: Option<Word>,
        block_header: BlockHeader,
        block_chain: PartialBlockchain,
        input_notes: InputNotes<InputNote>,
    ) -> Result<Self, TransactionInputError> {
        let partial_account: PartialAccount = account.into();

        // validate the seed
        validate_account_seed(&partial_account, account_seed)?;

        // check the block_chain and block_header are consistent
        let block_num = block_header.block_num();
        if block_chain.chain_length() != block_header.block_num() {
            return Err(TransactionInputError::InconsistentChainLength {
                expected: block_header.block_num(),
                actual: block_chain.chain_length(),
            });
        }

        if block_chain.peaks().hash_peaks() != block_header.chain_commitment() {
            return Err(TransactionInputError::InconsistentChainCommitment {
                expected: block_header.chain_commitment(),
                actual: block_chain.peaks().hash_peaks(),
            });
        }

        // check the authentication paths of the input notes.
        for note in input_notes.iter() {
            if let InputNote::Authenticated { note, proof } = note {
                let note_block_num = proof.location().block_num();

                let block_header = if note_block_num == block_num {
                    &block_header
                } else {
                    block_chain.get_block(note_block_num).ok_or(
                        TransactionInputError::InputNoteBlockNotInPartialBlockchain(note.id()),
                    )?
                };

                validate_is_in_block(note, proof, block_header)?;
            }
        }

        Ok(Self {
            account: partial_account,
            account_seed,
            block_header,
            block_chain,
            input_notes,
        })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the account against which the transaction is executed.
    pub fn account(&self) -> &PartialAccount {
        &self.account
    }

    /// For newly-created accounts, returns the account seed; for existing accounts, returns None.
    pub fn account_seed(&self) -> Option<Word> {
        self.account_seed
    }

    /// Returns block header for the block referenced by the transaction.
    pub fn block_header(&self) -> &BlockHeader {
        &self.block_header
    }

    /// Returns partial blockchain containing authentication paths for all notes consumed by the
    /// transaction.
    pub fn blockchain(&self) -> &PartialBlockchain {
        &self.block_chain
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
    ) -> (
        PartialAccount,
        Option<Word>,
        BlockHeader,
        PartialBlockchain,
        InputNotes<InputNote>,
    ) {
        (
            self.account,
            self.account_seed,
            self.block_header,
            self.block_chain,
            self.input_notes,
        )
    }
}

impl Serializable for TransactionInputs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account.write_into(target);
        self.account_seed.write_into(target);
        self.block_header.write_into(target);
        self.block_chain.write_into(target);
        self.input_notes.write_into(target);
    }
}

impl Deserializable for TransactionInputs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let partial_account = PartialAccount::read_from(source)?;
        let account_seed = source.read()?;
        let block_header = BlockHeader::read_from(source)?;
        let block_chain = PartialBlockchain::read_from(source)?;
        let input_notes = InputNotes::read_from(source)?;
        Self::new(partial_account, account_seed, block_header, block_chain, input_notes)
            .map_err(|err| DeserializationError::InvalidValue(format!("{err}")))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Validates that the provided seed is valid for this account.
fn validate_account_seed(
    account: &PartialAccount,
    account_seed: Option<Word>,
) -> Result<(), TransactionInputError> {
    match (account.is_new(), account_seed) {
        (true, Some(seed)) => {
            let account_id = AccountId::new(
                seed,
                account.id().version(),
                account.code().commitment(),
                account.storage().commitment(),
            )
            .map_err(TransactionInputError::InvalidAccountIdSeed)?;

            if account_id != account.id() {
                return Err(TransactionInputError::InconsistentAccountSeed {
                    expected: account.id(),
                    actual: account_id,
                });
            }

            Ok(())
        },
        (true, None) => Err(TransactionInputError::AccountSeedNotProvidedForNewAccount),
        (false, Some(_)) => Err(TransactionInputError::AccountSeedProvidedForExistingAccount),
        (false, None) => Ok(()),
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
