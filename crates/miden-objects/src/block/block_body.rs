use alloc::vec::Vec;

use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};

use crate::block::{BlockAccountUpdate, OutputNoteBatch};
use crate::note::Nullifier;
use crate::transaction::OrderedTransactionHeaders;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockBody {
    /// Account updates for the block.
    updated_accounts: Vec<BlockAccountUpdate>,

    /// Note batches created by the transactions in this block.
    output_note_batches: Vec<OutputNoteBatch>,

    /// Nullifiers created by the transactions in this block through the consumption of notes.
    created_nullifiers: Vec<Nullifier>,

    /// The aggregated and flattened transaction headers of all batches in the order in which they
    /// appeared in the proposed block.
    transactions: OrderedTransactionHeaders,
}

impl BlockBody {
    pub fn new(
        updated_accounts: Vec<BlockAccountUpdate>,
        output_note_batches: Vec<OutputNoteBatch>,
        created_nullifiers: Vec<Nullifier>,
        transactions: OrderedTransactionHeaders,
    ) -> Self {
        Self {
            updated_accounts,
            output_note_batches,
            created_nullifiers,
            transactions,
        }
    }

    /// Returns the slice of [`BlockAccountUpdate`]s for all accounts updated in the block.
    pub fn updated_accounts(&self) -> &[BlockAccountUpdate] {
        &self.updated_accounts
    }

    /// Returns the slice of [`OutputNoteBatch`]es for all output notes created in the block.
    pub fn output_note_batches(&self) -> &[OutputNoteBatch] {
        &self.output_note_batches
    }

    /// Returns a reference to the slice of nullifiers for all notes consumed in the block.
    pub fn created_nullifiers(&self) -> &[Nullifier] {
        &self.created_nullifiers
    }

    /// Returns the [`OrderedTransactionHeaders`] of all transactions included in this block.
    pub fn transactions(&self) -> &OrderedTransactionHeaders {
        &self.transactions
    }

    /// Returns a mutable reference to the block's account updates for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn updated_accounts_mut(&mut self) -> &mut Vec<BlockAccountUpdate> {
        &mut self.updated_accounts
    }

    /// Returns a mutable reference to the block's nullifiers for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn created_nullifiers_mut(&mut self) -> &mut Vec<Nullifier> {
        &mut self.created_nullifiers
    }

    /// Returns a mutable reference to the block's output note batches for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn output_note_batches_mut(&mut self) -> &mut Vec<OutputNoteBatch> {
        &mut self.output_note_batches
    }

    /// Consumes the block body and returns its parts.
    pub fn into_parts(
        self,
    ) -> (
        Vec<BlockAccountUpdate>,
        Vec<OutputNoteBatch>,
        Vec<Nullifier>,
        OrderedTransactionHeaders,
    ) {
        (
            self.updated_accounts,
            self.output_note_batches,
            self.created_nullifiers,
            self.transactions,
        )
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for BlockBody {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.updated_accounts.write_into(target);
        self.output_note_batches.write_into(target);
        self.created_nullifiers.write_into(target);
        self.transactions.write_into(target);
    }
}

impl Deserializable for BlockBody {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            updated_accounts: Vec::read_from(source)?,
            output_note_batches: Vec::read_from(source)?,
            created_nullifiers: Vec::read_from(source)?,
            transactions: OrderedTransactionHeaders::read_from(source)?,
        };
        Ok(block)
    }
}
