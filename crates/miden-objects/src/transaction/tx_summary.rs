use alloc::vec::Vec;

use crate::{
    Felt, Word,
    account::AccountDelta,
    crypto::SequentialCommit,
    transaction::{InputNote, InputNotes, OutputNotes},
    utils::{Deserializable, Serializable},
};

/// The summary of the changes that result from executing a transaction.
///
/// These are the account delta and the consumed and created notes. Because this data is intended to
/// be used for signing a transaction, replay protection is included as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionSummary {
    account_delta: AccountDelta,
    input_notes: InputNotes<InputNote>,
    output_notes: OutputNotes,
    replay_protection: Word,
}

impl TransactionSummary {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionSummary`] from the provided parts.
    pub fn new(
        account_delta: AccountDelta,
        input_notes: InputNotes<InputNote>,
        output_notes: OutputNotes,
        replay_protection: Word,
    ) -> Self {
        Self {
            account_delta,
            input_notes,
            output_notes,
            replay_protection,
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the account delta of this transaction summary.
    pub fn account_delta(&self) -> &AccountDelta {
        &self.account_delta
    }

    /// Returns the input notes of this transaction summary.
    pub fn input_notes(&self) -> &InputNotes<InputNote> {
        &self.input_notes
    }

    /// Returns the output notes of this transaction summary.
    pub fn output_notes(&self) -> &OutputNotes {
        &self.output_notes
    }

    /// Returns the replay protection word of this transaction summary.
    pub fn replay_protection(&self) -> Word {
        self.replay_protection
    }

    /// Computes the commitment to the [`TransactionSummary`].
    ///
    /// This can be used to sign the transaction.
    pub fn to_commitment(&self) -> Word {
        <Self as SequentialCommit>::to_commitment(self)
    }
}

impl SequentialCommit for TransactionSummary {
    type Commitment = Word;

    fn to_elements(&self) -> Vec<Felt> {
        let mut elements = Vec::with_capacity(16);
        elements.extend_from_slice(self.account_delta.commitment().as_elements());
        elements.extend_from_slice(self.input_notes.commitment().as_elements());
        elements.extend_from_slice(self.output_notes.commitment().as_elements());
        elements.extend_from_slice(self.replay_protection.as_elements());
        elements
    }
}

impl Serializable for TransactionSummary {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        self.account_delta.write_into(target);
        self.input_notes.write_into(target);
        self.output_notes.write_into(target);
        self.replay_protection.write_into(target);
    }
}

impl Deserializable for TransactionSummary {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let account_delta = source.read()?;
        let input_notes = source.read()?;
        let output_notes = source.read()?;
        let replay_protection = source.read()?;

        Ok(Self::new(account_delta, input_notes, output_notes, replay_protection))
    }
}
