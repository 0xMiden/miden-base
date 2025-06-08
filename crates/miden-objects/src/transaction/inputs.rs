use alloc::{collections::BTreeSet, vec::Vec};
use core::fmt::Debug;

use miden_crypto::merkle::{SmtProof, SmtProofError};

use super::{BlockHeader, Digest, Felt, Hasher, PartialBlockchain, Word};
use crate::{
    MAX_INPUT_NOTES_PER_TX, TransactionInputError,
    account::{Account, AccountCode, AccountId, PartialAccount, PartialStorage},
    asset::PartialVault,
    block::AccountWitness,
    note::{Note, NoteId, NoteInclusionProof, NoteLocation, Nullifier},
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
};

// TRANSACTION INPUTS
// ================================================================================================

/// Contains the data required to execute a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionInputs {
    account: Account,
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
        account: Account,
        account_seed: Option<Word>,
        block_header: BlockHeader,
        block_chain: PartialBlockchain,
        input_notes: InputNotes<InputNote>,
    ) -> Result<Self, TransactionInputError> {
        // validate the seed
        validate_account_seed(&account, account_seed)?;

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
            account,
            account_seed,
            block_header,
            block_chain,
            input_notes,
        })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns account against which the transaction is to be executed.
    pub fn account(&self) -> &Account {
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
    ) -> (Account, Option<Word>, BlockHeader, PartialBlockchain, InputNotes<InputNote>) {
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
        let account = Account::read_from(source)?;
        let account_seed = source.read()?;
        let block_header = BlockHeader::read_from(source)?;
        let block_chain = PartialBlockchain::read_from(source)?;
        let input_notes = InputNotes::read_from(source)?;
        Self::new(account, account_seed, block_header, block_chain, input_notes)
            .map_err(|err| DeserializationError::InvalidValue(format!("{}", err)))
    }
}

// TO INPUT NOTE COMMITMENT
// ================================================================================================

/// Specifies the data used by the transaction kernel to commit to a note.
///
/// The commitment is composed of:
///
/// - nullifier, which prevents double spend and provides unlinkability.
/// - an optional note commitment, which allows for delayed note authentication.
pub trait ToInputNoteCommitments {
    fn nullifier(&self) -> Nullifier;
    fn note_commitment(&self) -> Option<Digest>;
}

// INPUT NOTES
// ================================================================================================

/// Input notes for a transaction, empty if the transaction does not consume notes.
///
/// This structure is generic over `T`, so it can be used to create the input notes for transaction
/// execution, which require the note's details to run the transaction kernel, and the input notes
/// for proof verification, which require only the commitment data.
#[derive(Debug, Clone)]
pub struct InputNotes<T> {
    notes: Vec<T>,
    commitment: Digest,
}

impl<T: ToInputNoteCommitments> InputNotes<T> {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [InputNotes] instantiated from the provided vector of notes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The total number of notes is greater than [`MAX_INPUT_NOTES_PER_TX`].
    /// - The vector of notes contains duplicates.
    pub fn new(notes: Vec<T>) -> Result<Self, TransactionInputError> {
        if notes.len() > MAX_INPUT_NOTES_PER_TX {
            return Err(TransactionInputError::TooManyInputNotes(notes.len()));
        }

        let mut seen_notes = BTreeSet::new();
        for note in notes.iter() {
            if !seen_notes.insert(note.nullifier().inner()) {
                return Err(TransactionInputError::DuplicateInputNote(note.nullifier()));
            }
        }

        let commitment = build_input_note_commitment(&notes);

        Ok(Self { notes, commitment })
    }

    /// Returns new [`InputNotes`] instantiated from the provided vector of notes without checking
    /// their validity.
    ///
    /// This is exposed for use in transaction batches, but should generally not be used.
    ///
    /// # Warning
    ///
    /// This does not run the checks from [`InputNotes::new`], so the latter should be preferred.
    pub fn new_unchecked(notes: Vec<T>) -> Self {
        let commitment = build_input_note_commitment(&notes);
        Self { notes, commitment }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a sequential hash of nullifiers for all notes.
    ///
    /// For non empty lists the commitment is defined as:
    ///
    /// > hash(nullifier_0 || noteid0_or_zero || nullifier_1 || noteid1_or_zero || .. || nullifier_n
    /// > || noteidn_or_zero)
    ///
    /// Otherwise defined as ZERO for empty lists.
    pub fn commitment(&self) -> Digest {
        self.commitment
    }

    /// Returns total number of input notes.
    pub fn num_notes(&self) -> u16 {
        self.notes
            .len()
            .try_into()
            .expect("by construction, number of notes fits into u16")
    }

    /// Returns true if this [InputNotes] does not contain any notes.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Returns a reference to the note located at the specified index.
    pub fn get_note(&self, idx: usize) -> &T {
        &self.notes[idx]
    }

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over notes in this [InputNotes].
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.notes.iter()
    }

    // CONVERSIONS
    // --------------------------------------------------------------------------------------------

    /// Converts self into a vector of input notes.
    pub fn into_vec(self) -> Vec<T> {
        self.notes
    }
}

impl<T> IntoIterator for InputNotes<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.notes.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a InputNotes<T> {
    type Item = &'a T;
    type IntoIter = alloc::slice::Iter<'a, T>;

    fn into_iter(self) -> alloc::slice::Iter<'a, T> {
        self.notes.iter()
    }
}

impl<T: PartialEq> PartialEq for InputNotes<T> {
    fn eq(&self, other: &Self) -> bool {
        self.notes == other.notes
    }
}

impl<T: Eq> Eq for InputNotes<T> {}

impl<T: ToInputNoteCommitments> Default for InputNotes<T> {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            commitment: build_input_note_commitment::<T>(&[]),
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl<T: Serializable> Serializable for InputNotes<T> {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        // assert is OK here because we enforce max number of notes in the constructor
        assert!(self.notes.len() <= u16::MAX.into());
        target.write_u16(self.notes.len() as u16);
        target.write_many(&self.notes);
    }
}

impl<T: Deserializable + ToInputNoteCommitments> Deserializable for InputNotes<T> {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_notes = source.read_u16()?;
        let notes = source.read_many::<T>(num_notes.into())?;
        Self::new(notes).map_err(|err| DeserializationError::InvalidValue(format!("{}", err)))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

fn build_input_note_commitment<T: ToInputNoteCommitments>(notes: &[T]) -> Digest {
    // Note: This implementation must be kept in sync with the kernel's `process_input_notes_data`
    if notes.is_empty() {
        return Digest::default();
    }

    let mut elements: Vec<Felt> = Vec::with_capacity(notes.len() * 2);
    for commitment_data in notes {
        let nullifier = commitment_data.nullifier();
        let empty_word_or_note_commitment = &commitment_data
            .note_commitment()
            .map_or(Word::default(), |note_id| note_id.into());

        elements.extend_from_slice(nullifier.as_elements());
        elements.extend_from_slice(empty_word_or_note_commitment);
    }
    Hasher::hash_elements(&elements)
}

// INPUT NOTE
// ================================================================================================

const AUTHENTICATED: u8 = 0;
const UNAUTHENTICATED: u8 = 1;

/// An input note for a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputNote {
    /// Input notes whose existences in the chain is verified by the transaction kernel.
    Authenticated { note: Note, proof: NoteInclusionProof },

    /// Input notes whose existence in the chain is not verified by the transaction kernel, but
    /// instead is delegated to the protocol kernels.
    Unauthenticated { note: Note },
}

impl InputNote {
    // CONSTRUCTORS
    // -------------------------------------------------------------------------------------------

    /// Returns an authenticated [InputNote].
    pub fn authenticated(note: Note, proof: NoteInclusionProof) -> Self {
        Self::Authenticated { note, proof }
    }

    /// Returns an unauthenticated [InputNote].
    pub fn unauthenticated(note: Note) -> Self {
        Self::Unauthenticated { note }
    }

    // ACCESSORS
    // -------------------------------------------------------------------------------------------

    /// Returns the ID of the note.
    pub fn id(&self) -> NoteId {
        self.note().id()
    }

    /// Returns a reference to the underlying note.
    pub fn note(&self) -> &Note {
        match self {
            Self::Authenticated { note, .. } => note,
            Self::Unauthenticated { note } => note,
        }
    }

    /// Returns a reference to the inclusion proof of the note.
    pub fn proof(&self) -> Option<&NoteInclusionProof> {
        match self {
            Self::Authenticated { proof, .. } => Some(proof),
            Self::Unauthenticated { .. } => None,
        }
    }

    /// Returns a reference to the location of the note.
    pub fn location(&self) -> Option<&NoteLocation> {
        self.proof().map(|proof| proof.location())
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

impl ToInputNoteCommitments for InputNote {
    fn nullifier(&self) -> Nullifier {
        self.note().nullifier()
    }

    fn note_commitment(&self) -> Option<Digest> {
        match self {
            InputNote::Authenticated { .. } => None,
            InputNote::Unauthenticated { note } => Some(note.commitment()),
        }
    }
}

impl ToInputNoteCommitments for &InputNote {
    fn nullifier(&self) -> Nullifier {
        (*self).nullifier()
    }

    fn note_commitment(&self) -> Option<Digest> {
        (*self).note_commitment()
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for InputNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            Self::Authenticated { note, proof } => {
                target.write(AUTHENTICATED);
                target.write(note);
                target.write(proof);
            },
            Self::Unauthenticated { note } => {
                target.write(UNAUTHENTICATED);
                target.write(note);
            },
        }
    }
}

impl Deserializable for InputNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read_u8()? {
            AUTHENTICATED => {
                let note = Note::read_from(source)?;
                let proof = NoteInclusionProof::read_from(source)?;
                Ok(Self::Authenticated { note, proof })
            },
            UNAUTHENTICATED => {
                let note = Note::read_from(source)?;
                Ok(Self::Unauthenticated { note })
            },
            v => Err(DeserializationError::InvalidValue(format!("invalid input note type: {v}"))),
        }
    }
}

// INPUT NOTE
// ================================================================================================

/// Validates that the provided seed is valid for this account.
pub fn validate_account_seed(
    account: &Account,
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

// ACCOUNT INPUTS
// ================================================================================================

/// Contains information about an account, with everything required to execute a transaction.
///
/// `AccountInputs` combines a partial account representation with the merkle proof that verifies
/// the account's inclusion in the account tree. The partial account should contain verifiable
/// access to the parts of the state of the account of which the transaction will make use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountInputs {
    /// Partial representation of the account's state.
    partial_account: PartialAccount,
    /// Proof of the account's inclusion in the account tree for this account's state commitment.
    witness: AccountWitness,
}

impl AccountInputs {
    /// Creates a new instance of `AccountInputs` with the specified partial account and witness.
    pub fn new(partial_account: PartialAccount, witness: AccountWitness) -> AccountInputs {
        AccountInputs { partial_account, witness }
    }

    /// Returns the account ID.
    pub fn id(&self) -> AccountId {
        self.partial_account.id()
    }

    /// Returns a reference to the partial account representation.
    pub fn account(&self) -> &PartialAccount {
        &self.partial_account
    }

    /// Returns a reference to the account code.
    pub fn code(&self) -> &AccountCode {
        self.partial_account.code()
    }

    /// Returns a reference to the partial representation of the account storage.
    pub fn storage(&self) -> &PartialStorage {
        self.partial_account.storage()
    }

    /// Returns a reference to the partial vault representation of the account.
    pub fn vault(&self) -> &PartialVault {
        self.partial_account.vault()
    }

    /// Returns a reference to the account's witness.
    pub fn witness(&self) -> &AccountWitness {
        &self.witness
    }

    /// Decomposes the `AccountInputs` into its constituent parts.
    pub fn into_parts(self) -> (PartialAccount, AccountWitness) {
        (self.partial_account, self.witness)
    }

    /// Computes the account root based on the account witness.
    /// This root should be equal to the account root in the reference block header.
    pub fn compute_account_root(&self) -> Result<Digest, SmtProofError> {
        let smt_merkle_path = self.witness.path().clone();
        let smt_leaf = self.witness.leaf();
        let root = SmtProof::new(smt_merkle_path, smt_leaf)?.compute_root();

        Ok(root)
    }
}

impl Serializable for AccountInputs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(&self.partial_account);
        target.write(&self.witness);
    }
}

impl Deserializable for AccountInputs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let partial_account = source.read()?;
        let witness = source.read()?;

        Ok(AccountInputs { partial_account, witness })
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use miden_crypto::merkle::MerklePath;
    use vm_core::{
        Felt,
        utils::{Deserializable, Serializable},
    };
    use vm_processor::SMT_DEPTH;

    use crate::{
        account::{Account, AccountCode, AccountId, AccountStorage},
        asset::AssetVault,
        block::AccountWitness,
        testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        transaction::AccountInputs,
    };

    #[test]
    fn serde_roundtrip() {
        let id = AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap();
        let code = AccountCode::mock();
        let vault = AssetVault::new(&[]).unwrap();
        let storage = AccountStorage::new(vec![]).unwrap();
        let account = Account::from_parts(id, vault, storage, code, Felt::new(10));

        let commitment = account.commitment();

        let mut merkle_nodes = Vec::with_capacity(SMT_DEPTH as usize);
        for _ in 0..(SMT_DEPTH as usize) {
            merkle_nodes.push(commitment);
        }
        let merkle_path = MerklePath::new(merkle_nodes);

        let fpi_inputs = AccountInputs::new(
            account.into(),
            AccountWitness::new(id, commitment, merkle_path).unwrap(),
        );

        let serialized = fpi_inputs.to_bytes();
        let deserialized = AccountInputs::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, fpi_inputs);
    }
}
