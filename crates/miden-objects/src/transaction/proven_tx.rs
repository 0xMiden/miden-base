use alloc::string::ToString;
use alloc::vec::Vec;

use super::{InputNote, ToInputNoteCommitments};
use crate::account::delta::AccountUpdateDetails;
use crate::asset::FungibleAsset;
use crate::block::BlockNumber;
use crate::note::NoteHeader;
use crate::transaction::{
    AccountId,
    InputNotes,
    Nullifier,
    OutputNote,
    OutputNotes,
    TransactionId,
};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::vm::ExecutionProof;
use crate::{ACCOUNT_UPDATE_MAX_SIZE, EMPTY_WORD, ProvenTransactionError, Word};

// PROVEN TRANSACTION
// ================================================================================================

/// Result of executing and proving a transaction. Contains all the data required to verify that a
/// transaction was executed correctly.
///
/// A proven transaction must not be empty. A transaction is empty if the account state is unchanged
/// or the number of input notes is zero. This check prevents proving a transaction once and
/// submitting it to the network many times. Output notes are not considered because they can be
/// empty (i.e. contain no assets). Otherwise, a transaction with no account state change, no input
/// notes and one such empty output note could be resubmitted many times to the network and fill up
/// block space which is a form of DOS attack.
///
/// Instances are guaranteed to be valid as they are created via [`ProvenTransaction::new`], which
/// performs validation inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenTransaction {
    /// A unique identifier for the transaction, see [TransactionId] for additional details.
    id: TransactionId,

    /// Account update data.
    account_update: TxAccountUpdate,

    /// Committed details of all notes consumed by the transaction.
    input_notes: InputNotes<InputNoteCommitment>,

    /// Notes created by the transaction. For private notes, this will contain only note headers,
    /// while for public notes this will also contain full note details.
    output_notes: OutputNotes,

    /// [`BlockNumber`] of the transaction's reference block.
    ref_block_num: BlockNumber,

    /// The block commitment of the transaction's reference block.
    ref_block_commitment: Word,

    /// The fee of the transaction.
    fee: FungibleAsset,

    /// The block number by which the transaction will expire, as defined by the executed scripts.
    expiration_block_num: BlockNumber,

    /// A STARK proof that attests to the correct execution of the transaction.
    proof: ExecutionProof,
}

impl ProvenTransaction {
    /// Returns unique identifier of this transaction.
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// Returns ID of the account against which this transaction was executed.
    pub fn account_id(&self) -> AccountId {
        self.account_update.account_id()
    }

    /// Returns the account update details.
    pub fn account_update(&self) -> &TxAccountUpdate {
        &self.account_update
    }

    /// Returns a reference to the notes consumed by the transaction.
    pub fn input_notes(&self) -> &InputNotes<InputNoteCommitment> {
        &self.input_notes
    }

    /// Returns a reference to the notes produced by the transaction.
    pub fn output_notes(&self) -> &OutputNotes {
        &self.output_notes
    }

    /// Returns the proof of the transaction.
    pub fn proof(&self) -> &ExecutionProof {
        &self.proof
    }

    /// Returns the number of the reference block the transaction was executed against.
    pub fn ref_block_num(&self) -> BlockNumber {
        self.ref_block_num
    }

    /// Returns the commitment of the block transaction was executed against.
    pub fn ref_block_commitment(&self) -> Word {
        self.ref_block_commitment
    }

    /// Returns the fee of the transaction.
    pub fn fee(&self) -> FungibleAsset {
        self.fee
    }

    /// Returns an iterator of the headers of unauthenticated input notes in this transaction.
    pub fn unauthenticated_notes(&self) -> impl Iterator<Item = &NoteHeader> {
        self.input_notes.iter().filter_map(|note| note.header())
    }

    /// Returns the block number at which the transaction will expire.
    pub fn expiration_block_num(&self) -> BlockNumber {
        self.expiration_block_num
    }

    /// Returns an iterator over the nullifiers of all input notes in this transaction.
    ///
    /// This includes both authenticated and unauthenticated notes.
    pub fn nullifiers(&self) -> impl Iterator<Item = Nullifier> + '_ {
        self.input_notes.iter().map(InputNoteCommitment::nullifier)
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`ProvenTransaction`] with the provided parameters.
    ///
    /// This constructor performs validation inline to ensure the transaction is valid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The total number of input notes is greater than
    ///   [`MAX_INPUT_NOTES_PER_TX`](crate::constants::MAX_INPUT_NOTES_PER_TX).
    /// - The vector of input notes contains duplicates.
    /// - The total number of output notes is greater than
    ///   [`MAX_OUTPUT_NOTES_PER_TX`](crate::constants::MAX_OUTPUT_NOTES_PER_TX).
    /// - The vector of output notes contains duplicates.
    /// - The size of the serialized account update exceeds [`ACCOUNT_UPDATE_MAX_SIZE`].
    /// - The transaction is empty, which is the case if the account state is unchanged or the
    ///   number of input notes is zero.
    /// - The transaction was executed against a _new_ account with public state and its account ID
    ///   does not match the ID in the account update.
    /// - The transaction was executed against a _new_ account with public state and its commitment
    ///   does not match the final state commitment of the account update.
    /// - The transaction was executed against a private account and the account update is _not_ of
    ///   type [`AccountUpdateDetails::Private`].
    /// - The transaction was executed against an account with public state and the update is of
    ///   type [`AccountUpdateDetails::Private`].
    /// - The transaction was executed against an _existing_ account with public state and the
    ///   update is of type [`AccountUpdateDetails::New`].
    /// - The transaction creates a _new_ account with public state and the update is of type
    ///   [`AccountUpdateDetails::Delta`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: AccountId,
        initial_account_commitment: Word,
        final_account_commitment: Word,
        account_delta_commitment: Word,
        account_update_details: AccountUpdateDetails,
        input_notes: impl IntoIterator<Item = InputNoteCommitment>,
        output_notes: impl IntoIterator<Item = OutputNote>,
        ref_block_num: BlockNumber,
        ref_block_commitment: Word,
        fee: FungibleAsset,
        expiration_block_num: BlockNumber,
        proof: ExecutionProof,
    ) -> Result<Self, ProvenTransactionError> {
        let input_notes_vec: Vec<_> = input_notes.into_iter().collect();
        let output_notes_vec: Vec<_> = output_notes.into_iter().collect();

        let input_notes =
            InputNotes::new(input_notes_vec).map_err(ProvenTransactionError::InputNotesError)?;
        let output_notes =
            OutputNotes::new(output_notes_vec).map_err(ProvenTransactionError::OutputNotesError)?;
        let id = TransactionId::new(
            initial_account_commitment,
            final_account_commitment,
            input_notes.commitment(),
            output_notes.commitment(),
        );
        let account_update = TxAccountUpdate::new(
            account_id,
            initial_account_commitment,
            final_account_commitment,
            account_delta_commitment,
            account_update_details,
        );

        let proven_transaction = Self {
            id,
            account_update,
            input_notes,
            output_notes,
            ref_block_num,
            ref_block_commitment,
            fee,
            expiration_block_num,
            proof,
        };

        proven_transaction.validate()
    }

    /// Validates the transaction.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The size of the serialized account update exceeds [`ACCOUNT_UPDATE_MAX_SIZE`].
    /// - The transaction is empty, which is the case if the account state is unchanged or the
    ///   number of input notes is zero.
    /// - The transaction was executed against a _new_ account with public state and its account ID
    ///   does not match the ID in the account update.
    /// - The transaction was executed against a _new_ account with public state and its commitment
    ///   does not match the final state commitment of the account update.
    /// - The transaction was executed against a private account and the account update is _not_ of
    ///   type [`AccountUpdateDetails::Private`].
    /// - The transaction was executed against an account with public state and the update is of
    ///   type [`AccountUpdateDetails::Private`].
    /// - The transaction was executed against an _existing_ account with public state and the
    ///   update is of type [`AccountUpdateDetails::New`].
    /// - The transaction creates a _new_ account with public state and the update is of type
    ///   [`AccountUpdateDetails::Delta`].
    fn validate(self) -> Result<Self, ProvenTransactionError> {
        // If the account's state is public, then the account update details must be present.
        if self.account_id().has_public_state() {
            self.account_update.validate()?;

            // check that either the account state was changed or at least one note was consumed,
            // otherwise this transaction is empty
            if self.account_update.initial_state_commitment()
                == self.account_update.final_state_commitment()
                && self.input_notes.commitment() == EMPTY_WORD
            {
                return Err(ProvenTransactionError::EmptyTransaction);
            }

            let is_new_account = self.account_update.initial_state_commitment() == Word::empty();
            match self.account_update.details() {
                AccountUpdateDetails::Private => {
                    return Err(ProvenTransactionError::PublicStateAccountMissingDetails(
                        self.account_id(),
                    ));
                },
                AccountUpdateDetails::New(account) => {
                    if !is_new_account {
                        return Err(
                            ProvenTransactionError::ExistingPublicStateAccountRequiresDeltaDetails(
                                self.account_id(),
                            ),
                        );
                    }
                    if account.id() != self.account_id() {
                        return Err(ProvenTransactionError::AccountIdMismatch {
                            tx_account_id: self.account_id(),
                            details_account_id: account.id(),
                        });
                    }
                    if account.commitment() != self.account_update.final_state_commitment() {
                        return Err(ProvenTransactionError::AccountFinalCommitmentMismatch {
                            tx_final_commitment: self.account_update.final_state_commitment(),
                            details_commitment: account.commitment(),
                        });
                    }
                },
                AccountUpdateDetails::Delta(_) => {
                    if is_new_account {
                        return Err(
                            ProvenTransactionError::NewPublicStateAccountRequiresFullDetails(
                                self.account_id(),
                            ),
                        );
                    }
                },
            }
        } else if !self.account_update.is_private() {
            return Err(ProvenTransactionError::PrivateAccountWithDetails(self.account_id()));
        }

        Ok(self)
    }
}

impl Serializable for ProvenTransaction {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account_update.write_into(target);
        self.input_notes.write_into(target);
        self.output_notes.write_into(target);
        self.ref_block_num.write_into(target);
        self.ref_block_commitment.write_into(target);
        self.fee.write_into(target);
        self.expiration_block_num.write_into(target);
        self.proof.write_into(target);
    }
}

impl Deserializable for ProvenTransaction {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let account_update = TxAccountUpdate::read_from(source)?;

        let input_notes = <InputNotes<InputNoteCommitment>>::read_from(source)?;
        let output_notes = OutputNotes::read_from(source)?;

        let ref_block_num = BlockNumber::read_from(source)?;
        let ref_block_commitment = Word::read_from(source)?;
        let fee = FungibleAsset::read_from(source)?;
        let expiration_block_num = BlockNumber::read_from(source)?;
        let proof = ExecutionProof::read_from(source)?;

        // Extract values from account_update
        let account_id = account_update.account_id();
        let initial_account_commitment = account_update.initial_state_commitment();
        let final_account_commitment = account_update.final_state_commitment();
        let account_delta_commitment = account_update.account_delta_commitment();
        let account_update_details = account_update.details().clone();

        // Extract input and output notes as vecs
        let input_notes_vec = input_notes.into_vec();
        let output_notes_vec: Vec<_> = output_notes.iter().cloned().collect();

        ProvenTransaction::new(
            account_id,
            initial_account_commitment,
            final_account_commitment,
            account_delta_commitment,
            account_update_details,
            input_notes_vec,
            output_notes_vec,
            ref_block_num,
            ref_block_commitment,
            fee,
            expiration_block_num,
            proof,
        )
        .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TRANSACTION ACCOUNT UPDATE
// ================================================================================================

/// Describes the changes made to the account state resulting from a transaction execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAccountUpdate {
    /// ID of the account updated by a transaction.
    account_id: AccountId,

    /// The commitment of the account before the transaction was executed.
    ///
    /// Set to `Word::empty()` for new accounts.
    init_state_commitment: Word,

    /// The commitment of the account state after the transaction was executed.
    final_state_commitment: Word,

    /// The commitment to the account delta resulting from the execution of the transaction.
    ///
    /// This must be the commitment to the account delta as computed by the transaction kernel in
    /// the epilogue (the "pre-fee" delta). Notably, this _excludes_ the automatically removed fee
    /// asset. The account delta possibly contained in [`AccountUpdateDetails`] _includes_ the
    /// _removed_ fee asset, so that it represents the full account delta of the transaction
    /// (the "post-fee" delta). This mismatch means that in order to validate the delta, the
    /// fee asset must be _added_ to the delta before checking its commitment against this
    /// field.
    account_delta_commitment: Word,

    /// A set of changes which can be applied the account's state prior to the transaction to
    /// get the account state after the transaction. For private accounts this is set to
    /// [AccountUpdateDetails::Private].
    details: AccountUpdateDetails,
}

impl TxAccountUpdate {
    /// Returns a new [TxAccountUpdate] instantiated from the specified components.
    pub const fn new(
        account_id: AccountId,
        init_state_commitment: Word,
        final_state_commitment: Word,
        account_delta_commitment: Word,
        details: AccountUpdateDetails,
    ) -> Self {
        Self {
            account_id,
            init_state_commitment,
            final_state_commitment,
            account_delta_commitment,
            details,
        }
    }

    /// Returns the ID of the updated account.
    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// Returns the commitment of the account before the transaction was executed.
    pub fn initial_state_commitment(&self) -> Word {
        self.init_state_commitment
    }

    /// Returns the commitment of the account after the transaction was executed.
    pub fn final_state_commitment(&self) -> Word {
        self.final_state_commitment
    }

    /// Returns the commitment to the account delta resulting from the execution of the transaction.
    pub fn account_delta_commitment(&self) -> Word {
        self.account_delta_commitment
    }

    /// Returns the description of the updates for public accounts.
    ///
    /// These descriptions can be used to build the new account state from the previous account
    /// state.
    pub fn details(&self) -> &AccountUpdateDetails {
        &self.details
    }

    /// Returns `true` if the account update details are for a private account.
    pub fn is_private(&self) -> bool {
        self.details.is_private()
    }

    /// Validates the following properties of the account update:
    ///
    /// - The size of the serialized account update does not exceed [`ACCOUNT_UPDATE_MAX_SIZE`].
    pub fn validate(&self) -> Result<(), ProvenTransactionError> {
        let account_update_size = self.details().get_size_hint();
        if account_update_size > ACCOUNT_UPDATE_MAX_SIZE as usize {
            Err(ProvenTransactionError::AccountUpdateSizeLimitExceeded {
                account_id: self.account_id(),
                update_size: account_update_size,
            })
        } else {
            Ok(())
        }
    }
}

impl Serializable for TxAccountUpdate {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account_id.write_into(target);
        self.init_state_commitment.write_into(target);
        self.final_state_commitment.write_into(target);
        self.account_delta_commitment.write_into(target);
        self.details.write_into(target);
    }
}

impl Deserializable for TxAccountUpdate {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        Ok(Self {
            account_id: AccountId::read_from(source)?,
            init_state_commitment: Word::read_from(source)?,
            final_state_commitment: Word::read_from(source)?,
            account_delta_commitment: Word::read_from(source)?,
            details: AccountUpdateDetails::read_from(source)?,
        })
    }
}

// INPUT NOTE COMMITMENT
// ================================================================================================

/// The commitment to an input note.
///
/// For notes authenticated by the transaction kernel, the commitment consists only of the note's
/// nullifier. For notes whose authentication is delayed to batch/block kernels, the commitment
/// also includes full note header (i.e., note ID and metadata).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputNoteCommitment {
    nullifier: Nullifier,
    header: Option<NoteHeader>,
}

impl InputNoteCommitment {
    /// Returns the nullifier of the input note committed to by this commitment.
    pub fn nullifier(&self) -> Nullifier {
        self.nullifier
    }

    /// Returns the header of the input committed to by this commitment.
    ///
    /// Note headers are present only for notes whose presence in the change has not yet been
    /// authenticated.
    pub fn header(&self) -> Option<&NoteHeader> {
        self.header.as_ref()
    }

    /// Returns true if this commitment is for a note whose presence in the chain has been
    /// authenticated.
    ///
    /// Authenticated notes are represented solely by their nullifiers and are missing the note
    /// header.
    pub fn is_authenticated(&self) -> bool {
        self.header.is_none()
    }
}

impl From<InputNote> for InputNoteCommitment {
    fn from(note: InputNote) -> Self {
        Self::from(&note)
    }
}

impl From<&InputNote> for InputNoteCommitment {
    fn from(note: &InputNote) -> Self {
        match note {
            InputNote::Authenticated { note, .. } => Self {
                nullifier: note.nullifier(),
                header: None,
            },
            InputNote::Unauthenticated { note } => Self {
                nullifier: note.nullifier(),
                header: Some(*note.header()),
            },
        }
    }
}

impl From<Nullifier> for InputNoteCommitment {
    fn from(nullifier: Nullifier) -> Self {
        Self { nullifier, header: None }
    }
}

impl ToInputNoteCommitments for InputNoteCommitment {
    fn nullifier(&self) -> Nullifier {
        self.nullifier
    }

    fn note_commitment(&self) -> Option<Word> {
        self.header.map(|header| header.commitment())
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for InputNoteCommitment {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.nullifier.write_into(target);
        self.header.write_into(target);
    }
}

impl Deserializable for InputNoteCommitment {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let nullifier = Nullifier::read_from(source)?;
        let header = <Option<NoteHeader>>::read_from(source)?;

        Ok(Self { nullifier, header })
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    use anyhow::Context;
    use miden_core::utils::Deserializable;
    use miden_verifier::ExecutionProof;
    use winter_rand_utils::rand_value;

    use super::ProvenTransaction;
    use crate::account::delta::AccountUpdateDetails;
    use crate::account::{
        AccountDelta,
        AccountId,
        AccountIdVersion,
        AccountStorageDelta,
        AccountStorageMode,
        AccountType,
        AccountVaultDelta,
        StorageMapDelta,
    };
    use crate::asset::FungibleAsset;
    use crate::block::BlockNumber;
    use crate::testing::account_id::{
        ACCOUNT_ID_PRIVATE_SENDER,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    };
    use crate::transaction::{InputNoteCommitment, OutputNote, TxAccountUpdate};
    use crate::utils::Serializable;
    use crate::{
        ACCOUNT_UPDATE_MAX_SIZE,
        EMPTY_WORD,
        LexicographicWord,
        ONE,
        ProvenTransactionError,
        Word,
    };

    fn check_if_sync<T: Sync>() {}
    fn check_if_send<T: Send>() {}

    /// [ProvenTransaction] being Sync is part of its public API and changing it is backwards
    /// incompatible.
    #[test]
    fn test_proven_transaction_is_sync() {
        check_if_sync::<ProvenTransaction>();
    }

    /// [ProvenTransaction] being Send is part of its public API and changing it is backwards
    /// incompatible.
    #[test]
    fn test_proven_transaction_is_send() {
        check_if_send::<ProvenTransaction>();
    }

    #[test]
    fn account_update_size_limit_not_exceeded() {
        // A small delta does not exceed the limit.
        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        let storage_delta = AccountStorageDelta::from_iters(
            [1, 2, 3, 4],
            [(2, Word::from([1, 1, 1, 1u32])), (3, Word::from([1, 1, 0, 1u32]))],
            [],
        );
        let delta = AccountDelta::new(account_id, storage_delta, AccountVaultDelta::default(), ONE)
            .unwrap();
        let details = AccountUpdateDetails::Delta(delta);
        TxAccountUpdate::new(
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap(),
            EMPTY_WORD,
            EMPTY_WORD,
            EMPTY_WORD,
            details,
        )
        .validate()
        .unwrap();
    }

    #[test]
    fn account_update_size_limit_exceeded() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        let mut map = BTreeMap::new();
        // The number of entries in the map required to exceed the limit.
        // We divide by each entry's size which consists of a key (digest) and a value (word), both
        // 32 bytes in size.
        let required_entries = ACCOUNT_UPDATE_MAX_SIZE / (2 * 32);
        for _ in 0..required_entries {
            map.insert(LexicographicWord::new(rand_value::<Word>()), rand_value::<Word>());
        }
        let storage_delta = StorageMapDelta::new(map);

        // A delta that exceeds the limit returns an error.
        let storage_delta = AccountStorageDelta::from_iters([], [], [(4, storage_delta)]);
        let delta = AccountDelta::new(account_id, storage_delta, AccountVaultDelta::default(), ONE)
            .unwrap();
        let details = AccountUpdateDetails::Delta(delta);
        let details_size = details.get_size_hint();

        let err = TxAccountUpdate::new(
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap(),
            EMPTY_WORD,
            EMPTY_WORD,
            EMPTY_WORD,
            details,
        )
        .validate()
        .unwrap_err();

        assert!(
            matches!(err, ProvenTransactionError::AccountUpdateSizeLimitExceeded { update_size, .. } if update_size == details_size)
        );
    }

    #[test]
    fn test_proven_tx_serde_roundtrip() -> anyhow::Result<()> {
        let account_id = AccountId::dummy(
            [1; 15],
            AccountIdVersion::Version0,
            AccountType::FungibleFaucet,
            AccountStorageMode::Private,
        );
        let initial_account_commitment =
            [2; 32].try_into().expect("failed to create initial account commitment");
        let final_account_commitment =
            [3; 32].try_into().expect("failed to create final account commitment");
        let account_delta_commitment =
            [4; 32].try_into().expect("failed to create account delta commitment");
        let ref_block_num = BlockNumber::from(1);
        let ref_block_commitment = Word::empty();
        let expiration_block_num = BlockNumber::from(2);
        let proof = ExecutionProof::new_dummy();

        let tx = ProvenTransaction::new(
            account_id,
            initial_account_commitment,
            final_account_commitment,
            account_delta_commitment,
            AccountUpdateDetails::Private,
            Vec::<InputNoteCommitment>::new(),
            Vec::<OutputNote>::new(),
            ref_block_num,
            ref_block_commitment,
            FungibleAsset::mock(42).unwrap_fungible(),
            expiration_block_num,
            proof,
        )
        .context("failed to create proven transaction")?;

        let deserialized = ProvenTransaction::read_from_bytes(&tx.to_bytes()).unwrap();

        assert_eq!(tx, deserialized);

        Ok(())
    }
}
