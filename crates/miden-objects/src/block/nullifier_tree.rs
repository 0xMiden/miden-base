use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_core::{EMPTY_WORD, Felt, Word};
#[cfg(feature = "std")]
use miden_crypto::merkle::{LargeSmt, LargeSmtError, SmtStorage};
use miden_crypto::merkle::{MerkleError, MutationSet, Smt, SmtProof};
use miden_processor::{DeserializationError, SMT_DEPTH};
use miden_protocol_macros::WordWrapper;

use crate::block::{BlockNumber, NullifierWitness};
use crate::errors::NullifierTreeError;
use crate::note::Nullifier;

// CONSTANTS
// ================================================================================================

/// The value of an unspent nullifier in the tree.
pub(super) const UNSPENT_NULLIFIER: Word = EMPTY_WORD;

/// A nullifier leaf value in the nullifier SMT.
///
/// # Invariants
///
/// Assumes the provided SMT upholds the guarantees of the [`NullifierLeafValue`]. Specifically:
/// - NullifierLeafValue must follow the format `Word([block_num, 0, 0, 0])` with `block_num` a
///   valid block number.
#[derive(WordWrapper, Debug, PartialEq, Eq)]
pub struct NullifierLeafValue(Word);

impl NullifierLeafValue {
    pub fn new(value: Word) -> Result<Self, NullifierTreeError> {
        if TryInto::<u32>::try_into(value[0].as_int()).is_ok()
            && value[1..4].iter().all(|l| l.inner() == 0)
        {
            return Ok(Self::from_raw(value));
        }
        Err(NullifierTreeError::InvalidValue)
    }

    /// Given the leaf value of the nullifier SMT, returns the nullifier's block number.
    ///
    /// There are no nullifiers in the genesis block. The value zero is instead used to signal
    /// absence of a value.
    pub fn as_block_num(&self) -> BlockNumber {
        let block_num: u32 =
            self.as_elements()[0].as_int().try_into().expect("invalid block number");

        block_num.into()
    }

    pub fn from_block_number(block_num: BlockNumber) -> Self {
        NullifierLeafValue::from_raw(Word::from([block_num.as_u32(), 0, 0, 0]))
    }
}

// NULLIFIER TREE BACKEND TRAIT
// ================================================================================================

/// This trait abstracts over different SMT backends (e.g., `Smt` and `LargeSmt`) to allow
/// the `NullifierTree` to work with either implementation transparently.
///
/// Users should instantiate the backend directly (potentially with entries) and then
/// pass it to [`NullifierTree::new_unchecked`].
///
/// # Invariants
///
/// Assumes the provided SMT upholds the guarantees of the [`NullifierTree`]. Specifically:
/// - Nullifiers are only spent once and their block numbers do not change.
/// - Nullifier leaf values must be valid according to [`NullifierLeafValue`].
pub trait NullifierTreeBackend: Sized {
    type Error: core::error::Error + Send + 'static;

    /// Returns the number of entries in the SMT.
    fn num_entries(&self) -> usize;

    /// Returns all entries in the SMT as an iterator over key-value pairs.
    fn entries(&self) -> Box<dyn Iterator<Item = (Word, Word)> + '_>;

    /// Opens the leaf at the given key, returning a Merkle proof.
    fn open(&self, key: &Word) -> SmtProof;

    /// Applies the given mutation set to the SMT.
    fn apply_mutations(
        &mut self,
        set: MutationSet<SMT_DEPTH, Word, Word>,
    ) -> Result<(), Self::Error>;

    /// Computes the mutation set required to apply the given updates to the SMT.
    fn compute_mutations(
        &self,
        updates: impl IntoIterator<Item = (Word, Word)>,
    ) -> Result<MutationSet<SMT_DEPTH, Word, Word>, Self::Error>;

    /// Inserts a key-value pair into the SMT, returning the previous value at that key.
    fn insert(&mut self, key: Word, value: NullifierLeafValue) -> Result<Word, Self::Error>;

    /// Returns the value associated with the given key.
    fn get_value(&self, key: &Word) -> NullifierLeafValue;

    /// Returns the root of the SMT.
    fn root(&self) -> Word;
}

impl NullifierTreeBackend for Smt {
    type Error = MerkleError;

    fn num_entries(&self) -> usize {
        Smt::num_entries(self)
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Word, Word)> + '_> {
        Box::new(Smt::entries(self).map(|(k, v)| (*k, *v)))
    }

    fn open(&self, key: &Word) -> SmtProof {
        Smt::open(self, key)
    }

    fn apply_mutations(
        &mut self,
        set: MutationSet<SMT_DEPTH, Word, Word>,
    ) -> Result<(), Self::Error> {
        Smt::apply_mutations(self, set)
    }

    fn compute_mutations(
        &self,
        updates: impl IntoIterator<Item = (Word, Word)>,
    ) -> Result<MutationSet<SMT_DEPTH, Word, Word>, Self::Error> {
        Smt::compute_mutations(self, updates)
    }

    fn insert(&mut self, key: Word, value: NullifierLeafValue) -> Result<Word, Self::Error> {
        Smt::insert(self, key, value.as_word())
    }

    fn get_value(&self, key: &Word) -> NullifierLeafValue {
        NullifierLeafValue::from_raw(Smt::get_value(self, key))
    }

    fn root(&self) -> Word {
        Smt::root(self)
    }
}

#[cfg(feature = "std")]
fn large_smt_error_to_merkle_error(err: LargeSmtError) -> MerkleError {
    match err {
        LargeSmtError::Storage(storage_err) => {
            panic!("Storage error encountered: {:?}", storage_err)
        },
        LargeSmtError::Merkle(merkle_err) => merkle_err,
    }
}

#[cfg(feature = "std")]
impl<Backend> NullifierTreeBackend for LargeSmt<Backend>
where
    Backend: SmtStorage,
{
    type Error = MerkleError;

    fn num_entries(&self) -> usize {
        // SAFETY: We panic on storage errors here as they represent unrecoverable I/O failures.
        // This maintains API compatibility with the non-fallible Smt::num_entries().
        // See issue #2010 for future improvements to error handling.
        LargeSmt::num_entries(self)
            .map_err(large_smt_error_to_merkle_error)
            .expect("Storage I/O error accessing num_entries")
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Word, Word)> + '_> {
        // SAFETY: We expect here as only I/O errors can occur. Storage failures are considered
        // unrecoverable at this layer. See issue #2010 for future error handling improvements.
        Box::new(LargeSmt::entries(self).expect("Storage I/O error accessing entries"))
    }

    fn open(&self, key: &Word) -> SmtProof {
        LargeSmt::open(self, key)
    }

    fn apply_mutations(
        &mut self,
        set: MutationSet<SMT_DEPTH, Word, Word>,
    ) -> Result<(), Self::Error> {
        LargeSmt::apply_mutations(self, set).map_err(large_smt_error_to_merkle_error)
    }

    fn compute_mutations(
        &self,
        updates: impl IntoIterator<Item = (Word, Word)>,
    ) -> Result<MutationSet<SMT_DEPTH, Word, Word>, Self::Error> {
        LargeSmt::compute_mutations(self, updates).map_err(large_smt_error_to_merkle_error)
    }

    fn insert(&mut self, key: Word, value: NullifierLeafValue) -> Result<Word, Self::Error> {
        LargeSmt::insert(self, key, value.as_word())
    }

    fn get_value(&self, key: &Word) -> NullifierLeafValue {
        NullifierLeafValue::from_raw(LargeSmt::get_value(self, key))
    }

    fn root(&self) -> Word {
        // SAFETY: We expect here as storage errors are considered unrecoverable. This maintains
        // API compatibility with the non-fallible Smt::root().
        // See issue #2010 for future improvements to error handling.
        LargeSmt::root(self)
            .map_err(large_smt_error_to_merkle_error)
            .expect("Storage I/O error accessing root")
    }
}

/// The sparse merkle tree of all nullifiers in the blockchain.
///
/// A nullifier can only ever be spent once and its value in the tree is the block number at which
/// it was spent.
///
/// The tree guarantees that once a nullifier has been inserted into the tree, its block number does
/// not change. Note that inserting the nullifier multiple times with the same block number is
/// valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullifierTree<Backend = Smt> {
    smt: Backend,
}

impl<Backend> Default for NullifierTree<Backend>
where
    Backend: Default,
{
    fn default() -> Self {
        Self { smt: Default::default() }
    }
}

impl<Backend> NullifierTree<Backend>
where
    Backend: NullifierTreeBackend<Error = MerkleError>,
{
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The depth of the nullifier tree.
    pub const DEPTH: u8 = SMT_DEPTH;

    /// The value of an unspent nullifier in the tree.
    pub const UNSPENT_NULLIFIER: Word = EMPTY_WORD;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new `NullifierTree` from its inner representation.
    ///
    /// # Invariants
    ///
    /// See the documentation on [`NullifierTreeBackend`] trait documentation.
    pub fn new_unchecked(backend: Backend) -> Self {
        NullifierTree { smt: backend }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the root of the nullifier SMT.
    pub fn root(&self) -> Word {
        self.smt.root()
    }

    /// Returns the number of spent nullifiers in this tree.
    pub fn num_nullifiers(&self) -> usize {
        self.smt.num_entries()
    }

    /// Returns an iterator over the nullifiers and their block numbers in the tree.
    pub fn entries(&self) -> impl Iterator<Item = (Nullifier, BlockNumber)> {
        self.smt.entries().map(|(nullifier, value)| {
            (
                Nullifier::from_raw(nullifier),
                NullifierLeafValue::from_raw(value).as_block_num(),
            )
        })
    }

    /// Returns a [`NullifierWitness`] of the leaf associated with the `nullifier`.
    ///
    /// Conceptually, such a witness is a Merkle path to the leaf, as well as the leaf itself.
    ///
    /// This witness is a proof of the current block number of the given nullifier. If that block
    /// number is zero, it proves that the nullifier is unspent.
    pub fn open(&self, nullifier: &Nullifier) -> NullifierWitness {
        NullifierWitness::new(self.smt.open(&nullifier.as_word()))
    }

    /// Returns the block number for the given nullifier or `None` if the nullifier wasn't spent
    /// yet.
    pub fn get_block_num(&self, nullifier: &Nullifier) -> Option<BlockNumber> {
        let nullifier_leaf_value = self.smt.get_value(&nullifier.as_word());
        if nullifier_leaf_value.as_word() == Self::UNSPENT_NULLIFIER {
            return None;
        }

        Some(nullifier_leaf_value.as_block_num())
    }

    /// Computes a mutation set resulting from inserting the provided nullifiers into this nullifier
    /// tree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - a nullifier in the provided iterator was already spent.
    pub fn compute_mutations<I>(
        &self,
        nullifiers: impl IntoIterator<Item = (Nullifier, BlockNumber), IntoIter = I>,
    ) -> Result<NullifierMutationSet, NullifierTreeError>
    where
        I: Iterator<Item = (Nullifier, BlockNumber)> + Clone,
    {
        let nullifiers = nullifiers.into_iter();
        for (nullifier, _) in nullifiers.clone() {
            if self.get_block_num(&nullifier).is_some() {
                return Err(NullifierTreeError::NullifierAlreadySpent(nullifier));
            }
        }

        let mutation_set = self
            .smt
            .compute_mutations(
                nullifiers
                    .into_iter()
                    .map(|(nullifier, block_num)| {
                        (
                            nullifier.as_word(),
                            NullifierLeafValue::from_block_number(block_num).as_word(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(NullifierTreeError::ComputeMutations)?;

        Ok(NullifierMutationSet::new(mutation_set))
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Marks the given nullifier as spent at the given block number.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the nullifier was already spent.
    pub fn mark_spent(
        &mut self,
        nullifier: Nullifier,
        block_num: BlockNumber,
    ) -> Result<(), NullifierTreeError> {
        let prev_nullifier_value = self
            .smt
            .insert(nullifier.as_word(), NullifierLeafValue::from_block_number(block_num))
            .map_err(NullifierTreeError::MaxLeafEntriesExceeded)?;

        if prev_nullifier_value != Self::UNSPENT_NULLIFIER {
            Err(NullifierTreeError::NullifierAlreadySpent(nullifier))
        } else {
            Ok(())
        }
    }

    /// Applies mutations to the nullifier tree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `mutations` was computed on a tree with a different root than this one.
    pub fn apply_mutations(
        &mut self,
        mutations: NullifierMutationSet,
    ) -> Result<(), NullifierTreeError> {
        self.smt
            .apply_mutations(mutations.into_mutation_set())
            .map_err(NullifierTreeError::TreeRootConflict)
    }
}

// CONVENIENCE METHODS
// ================================================================================================

impl NullifierTree<Smt> {
    /// Creates a new nullifier tree from the provided entries.
    ///
    /// This is a convenience method that creates an SMT backend with the provided entries and
    /// wraps it in a NullifierTree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided entries contain multiple block numbers for the same nullifier.
    pub fn with_entries(
        entries: impl IntoIterator<Item = (Nullifier, BlockNumber)>,
    ) -> Result<Self, NullifierTreeError> {
        let leaves = entries.into_iter().map(|(nullifier, block_num)| {
            (nullifier.as_word(), NullifierLeafValue::from_block_number(block_num).as_word())
        });

        let smt = Smt::with_entries(leaves)
            .map_err(NullifierTreeError::DuplicateNullifierBlockNumbers)?;

        Ok(Self::new_unchecked(smt))
    }
}

#[cfg(feature = "std")]
impl<Backend> NullifierTree<LargeSmt<Backend>>
where
    Backend: SmtStorage,
{
    /// Creates a new nullifier tree from the provided entries using the given storage backend
    ///
    /// This is a convenience method that creates an SMT on the provided storage backend using the
    /// provided entries and wraps it in a NullifierTree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided entries contain multiple block numbers for the same nullifier.
    /// - a storage error is encountered.
    pub fn with_storage_from_entries(
        storage: Backend,
        entries: impl IntoIterator<Item = (Nullifier, BlockNumber)>,
    ) -> Result<Self, NullifierTreeError> {
        let leaves = entries.into_iter().map(|(nullifier, block_num)| {
            (nullifier.as_word(), NullifierLeafValue::from_block_number(block_num).as_word())
        });

        let smt = LargeSmt::<Backend>::with_entries(storage, leaves)
            .map_err(large_smt_error_to_merkle_error)
            .map_err(NullifierTreeError::DuplicateNullifierBlockNumbers)?;

        Ok(Self::new_unchecked(smt))
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NullifierTree {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.entries().collect::<Vec<_>>().write_into(target);
    }
}

impl Deserializable for NullifierTree {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let entries = Vec::<(Nullifier, BlockNumber)>::read_from(source)?;
        Self::with_entries(entries)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// NULLIFIER MUTATION SET
// ================================================================================================

/// A newtype wrapper around a [`MutationSet`] for use in the [`NullifierTree`].
///
/// It guarantees that applying the contained mutations will result in a nullifier tree where
/// nullifier's block numbers are not updated (except if they were unspent before), ensuring that
/// nullifiers are only spent once.
///
/// It is returned by and used in methods on the [`NullifierTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullifierMutationSet {
    mutation_set: MutationSet<SMT_DEPTH, Word, Word>,
}

impl NullifierMutationSet {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NullifierMutationSet`] from the provided raw mutation set.
    fn new(mutation_set: MutationSet<SMT_DEPTH, Word, Word>) -> Self {
        Self { mutation_set }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the underlying [`MutationSet`].
    pub fn as_mutation_set(&self) -> &MutationSet<SMT_DEPTH, Word, Word> {
        &self.mutation_set
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Consumes self and returns the underlying [`MutationSet`].
    pub fn into_mutation_set(self) -> MutationSet<SMT_DEPTH, Word, Word> {
        self.mutation_set
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::NullifierTree;
    use crate::block::BlockNumber;
    use crate::block::nullifier_tree::NullifierLeafValue;
    use crate::note::Nullifier;
    use crate::{NullifierTreeError, Word};

    #[test]
    fn leaf_value_encode_decode() {
        let block_num = BlockNumber::from(0xffff_ffff_u32);
        let leaf = NullifierLeafValue::from_block_number(block_num);
        let block_num_recovered = leaf.as_block_num();
        assert_eq!(block_num, block_num_recovered);
    }

    #[test]
    fn leaf_value_encoding() {
        let block_num = 123;
        let nullifier_value = NullifierLeafValue::from_block_number(block_num.into());
        assert_eq!(
            nullifier_value,
            NullifierLeafValue::from_raw(Word::from([block_num, 0, 0, 0u32]))
        );
    }

    #[test]
    fn leaf_value_decoding() {
        let block_num = 123;
        let nullifier_value = NullifierLeafValue::from_raw(Word::from([block_num, 0, 0, 0u32]));
        let decoded_block_num = nullifier_value.as_block_num();

        assert_eq!(decoded_block_num, block_num.into());
    }

    #[test]
    fn apply_mutations() {
        let nullifier1 = Nullifier::dummy(1);
        let nullifier2 = Nullifier::dummy(2);
        let nullifier3 = Nullifier::dummy(3);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);
        let block3 = BlockNumber::from(3);

        let mut tree = NullifierTree::with_entries([(nullifier1, block1)]).unwrap();

        // Check that passing nullifier2 twice with different values will use the last value.
        let mutations = tree
            .compute_mutations([(nullifier2, block1), (nullifier3, block3), (nullifier2, block2)])
            .unwrap();

        tree.apply_mutations(mutations).unwrap();

        assert_eq!(tree.num_nullifiers(), 3);
        assert_eq!(tree.get_block_num(&nullifier1).unwrap(), block1);
        assert_eq!(tree.get_block_num(&nullifier2).unwrap(), block2);
        assert_eq!(tree.get_block_num(&nullifier3).unwrap(), block3);
    }

    #[test]
    fn nullifier_already_spent() {
        let nullifier1 = Nullifier::dummy(1);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);

        let mut tree = NullifierTree::with_entries([(nullifier1, block1)]).unwrap();

        // Attempt to insert nullifier 1 again at _the same_ block number.
        let err = tree.clone().compute_mutations([(nullifier1, block1)]).unwrap_err();
        assert_matches!(err, NullifierTreeError::NullifierAlreadySpent(nullifier) if nullifier == nullifier1);

        let err = tree.clone().mark_spent(nullifier1, block1).unwrap_err();
        assert_matches!(err, NullifierTreeError::NullifierAlreadySpent(nullifier) if nullifier == nullifier1);

        // Attempt to insert nullifier 1 again at a different block number.
        let err = tree.clone().compute_mutations([(nullifier1, block2)]).unwrap_err();
        assert_matches!(err, NullifierTreeError::NullifierAlreadySpent(nullifier) if nullifier == nullifier1);

        let err = tree.mark_spent(nullifier1, block2).unwrap_err();
        assert_matches!(err, NullifierTreeError::NullifierAlreadySpent(nullifier) if nullifier == nullifier1);
    }

    #[cfg(feature = "std")]
    #[test]
    fn large_smt_backend_basic_operations() {
        use miden_crypto::merkle::{LargeSmt, MemoryStorage};

        // Create test data
        let nullifier1 = Nullifier::dummy(1);
        let nullifier2 = Nullifier::dummy(2);
        let nullifier3 = Nullifier::dummy(3);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);
        let block3 = BlockNumber::from(3);

        // Create NullifierTree with LargeSmt backend
        let mut tree = NullifierTree::new_unchecked(
            LargeSmt::with_entries(
                MemoryStorage::default(),
                [
                    (nullifier1.as_word(), NullifierLeafValue::from_block_number(block1).as_word()),
                    (nullifier2.as_word(), NullifierLeafValue::from_block_number(block2).as_word()),
                ],
            )
            .unwrap(),
        );

        // Test basic operations
        assert_eq!(tree.num_nullifiers(), 2);
        assert_eq!(tree.get_block_num(&nullifier1).unwrap(), block1);
        assert_eq!(tree.get_block_num(&nullifier2).unwrap(), block2);

        // Test opening
        let _witness1 = tree.open(&nullifier1);

        // Test mutations
        tree.mark_spent(nullifier3, block3).unwrap();
        assert_eq!(tree.num_nullifiers(), 3);
        assert_eq!(tree.get_block_num(&nullifier3).unwrap(), block3);
    }

    #[cfg(feature = "std")]
    #[test]
    fn large_smt_backend_nullifier_already_spent() {
        use miden_crypto::merkle::{LargeSmt, MemoryStorage};

        let nullifier1 = Nullifier::dummy(1);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);

        let mut tree = NullifierTree::new_unchecked(
            LargeSmt::with_entries(
                MemoryStorage::default(),
                [(nullifier1.as_word(), NullifierLeafValue::from_block_number(block1).as_word())],
            )
            .unwrap(),
        );

        assert_eq!(tree.get_block_num(&nullifier1).unwrap(), block1);

        let err = tree.mark_spent(nullifier1, block2).unwrap_err();
        assert_matches!(err, NullifierTreeError::NullifierAlreadySpent(nullifier) if nullifier == nullifier1);
    }

    #[cfg(feature = "std")]
    #[test]
    fn large_smt_backend_apply_mutations() {
        use miden_crypto::merkle::{LargeSmt, MemoryStorage};

        let nullifier1 = Nullifier::dummy(1);
        let nullifier2 = Nullifier::dummy(2);
        let nullifier3 = Nullifier::dummy(3);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);
        let block3 = BlockNumber::from(3);

        let mut tree = LargeSmt::with_entries(
            MemoryStorage::default(),
            [(nullifier1.as_word(), NullifierLeafValue::from_block_number(block1).as_word())],
        )
        .map(NullifierTree::new_unchecked)
        .unwrap();

        let mutations =
            tree.compute_mutations([(nullifier2, block2), (nullifier3, block3)]).unwrap();

        tree.apply_mutations(mutations).unwrap();

        assert_eq!(tree.num_nullifiers(), 3);
        assert_eq!(tree.get_block_num(&nullifier1).unwrap(), block1);
        assert_eq!(tree.get_block_num(&nullifier2).unwrap(), block2);
        assert_eq!(tree.get_block_num(&nullifier3).unwrap(), block3);
    }

    #[cfg(feature = "std")]
    #[test]
    fn large_smt_backend_same_root_as_regular_smt() {
        use miden_crypto::merkle::{LargeSmt, MemoryStorage};

        let nullifier1 = Nullifier::dummy(1);
        let nullifier2 = Nullifier::dummy(2);

        let block1 = BlockNumber::from(1);
        let block2 = BlockNumber::from(2);

        // Create tree with LargeSmt backend
        let large_tree = LargeSmt::with_entries(
            MemoryStorage::default(),
            [
                (nullifier1.as_word(), NullifierLeafValue::from_block_number(block1).as_word()),
                (nullifier2.as_word(), NullifierLeafValue::from_block_number(block2).as_word()),
            ],
        )
        .map(NullifierTree::new_unchecked)
        .unwrap();

        // Create tree with regular Smt backend
        let regular_tree =
            NullifierTree::with_entries([(nullifier1, block1), (nullifier2, block2)]).unwrap();

        // Both should have the same root
        assert_eq!(large_tree.root(), regular_tree.root());

        // Both should have the same nullifier entries
        let large_entries: std::collections::BTreeMap<_, _> = large_tree.entries().collect();
        let regular_entries: std::collections::BTreeMap<_, _> = regular_tree.entries().collect();

        assert_eq!(large_entries, regular_entries);
    }
}
