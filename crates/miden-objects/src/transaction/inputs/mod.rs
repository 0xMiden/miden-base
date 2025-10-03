use core::fmt::Debug;

use super::PartialBlockchain;
use crate::TransactionInputError;
use crate::account::PartialAccount;
use crate::block::BlockHeader;

mod account;
pub use account::AccountInputs;

mod notes;
pub use notes::{InputNote, InputNotes, ToInputNoteCommitments};

// TRANSACTION INPUTS
// ================================================================================================

/// Contains the data required to execute a transaction, minus the input notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionPreparationInputs {
    account: PartialAccount,
    block_header: BlockHeader,
    blockchain: PartialBlockchain,
}

impl TransactionPreparationInputs {
    /// Returns new [`TransactionPreparationInputs`] instantiated with the specified parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The partial blockchain's length is not the number of the reference block.
    /// - The partial blockchain's commitment does not match the reference block's chain commitment.
    pub fn new(
        account: PartialAccount,
        block_header: BlockHeader,
        blockchain: PartialBlockchain,
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

        Ok(Self { account, block_header, blockchain })
    }

    /// Returns a reference to the partial account.
    pub fn account(&self) -> &PartialAccount {
        &self.account
    }

    /// Returns a reference to the block header.
    pub fn block_header(&self) -> &BlockHeader {
        &self.block_header
    }

    /// Returns a reference to the partial blockchain.
    pub fn blockchain(&self) -> &PartialBlockchain {
        &self.blockchain
    }

    /// Consumes the [`TransactionPreparationInputs`] and returns its parts.
    pub fn into_parts(self) -> (PartialAccount, BlockHeader, PartialBlockchain) {
        (self.account, self.block_header, self.blockchain)
    }
}
