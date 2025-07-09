use vm_core::EMPTY_WORD;

use crate::{
    Felt, FieldElement, Word,
    block::{BlockNumber, NullifierWitness},
    crypto::merkle::{MutationSet, SMT_DEPTH, Smt},
    errors::NullifierTreeError,
    note::Nullifier,
};

/// The sparse merkle tree of all nullifiers in the blockchain.
///
/// A nullifier can only ever be spent once and its value in the tree is the block number at which
/// it was spent.
///
/// The tree guarantees that once a nullifier has been inserted into the tree, its block number does
/// not change. Note that inserting the nullifier multiple times with the same block number is
/// valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullifierTree {
    smt: Smt,
}

impl NullifierTree {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The depth of the nullifier tree.
    pub const DEPTH: u8 = SMT_DEPTH;

    /// The value of an unspent nullifier in the tree.
    pub const UNSPENT_NULLIFIER: Word = EMPTY_WORD;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new, empty nullifier tree.
    pub fn new() -> Self {
        Self { smt: Smt::new() }
    }

    /// Construct a new nullifier tree from the provided entries.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided entries contain multiple block numbers for the same nullifier.
    pub fn with_entries(
        entries: impl IntoIterator<Item = (Nullifier, BlockNumber)>,
    ) -> Result<Self, NullifierTreeError> {
        let leaves = entries.into_iter().map(|(nullifier, block_num)| {
            (nullifier.inner(), Self::block_num_to_leaf_value(block_num))
        });

        let smt = Smt::with_entries(leaves)
            .map_err(NullifierTreeError::DuplicateNullifierBlockNumbers)?;

        Ok(Self { smt })
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
        self.smt.entries().map(|(nullifier, block_num)| {
            (Nullifier::from(*nullifier), Self::leaf_value_to_block_num(*block_num))
        })
    }

    /// Returns a [`NullifierWitness`] of the leaf associated with the `nullifier`.
    ///
    /// Conceptually, such a witness is a Merkle path to the leaf, as well as the leaf itself.
    ///
    /// This witness is a proof of the current block number of the given nullifier. If that block
    /// number is zero, it proves that the nullifier is unspent.
    pub fn open(&self, nullifier: &Nullifier) -> NullifierWitness {
        NullifierWitness::new(self.smt.open(&nullifier.inner()))
    }

    /// Returns the block number for the given nullifier or `None` if the nullifier wasn't spent
    /// yet.
    pub fn get_block_num(&self, nullifier: &Nullifier) -> Option<BlockNumber> {
        let value = self.smt.get_value(&nullifier.inner());
        if value == Self::UNSPENT_NULLIFIER {
            return None;
        }

        Some(Self::leaf_value_to_block_num(value))
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

        let mutation_set =
            self.smt.compute_mutations(nullifiers.into_iter().map(|(nullifier, block_num)| {
                (nullifier.inner(), Self::block_num_to_leaf_value(block_num))
            }));

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
        let prev_nullifier_value =
            self.smt.insert(nullifier.inner(), Self::block_num_to_leaf_value(block_num));

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

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Returns the nullifier's leaf value in the SMT by its block number.
    pub(super) fn block_num_to_leaf_value(block: BlockNumber) -> Word {
        Word::from([Felt::from(block), Felt::ZERO, Felt::ZERO, Felt::ZERO])
    }

    /// Given the leaf value of the nullifier SMT, returns the nullifier's block number.
    ///
    /// There are no nullifiers in the genesis block. The value zero is instead used to signal
    /// absence of a value.
    fn leaf_value_to_block_num(value: Word) -> BlockNumber {
        let block_num: u32 =
            value[0].as_int().try_into().expect("invalid block number found in store");

        block_num.into()
    }
}

impl Default for NullifierTree {
    fn default() -> Self {
        Self::new()
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
    mutation_set: MutationSet<{ NullifierTree::DEPTH }, Word, Word>,
}

impl NullifierMutationSet {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`AccountMutationSet`] from the provided raw mutation set.
    fn new(mutation_set: MutationSet<{ NullifierTree::DEPTH }, Word, Word>) -> Self {
        Self { mutation_set }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the underlying [`MutationSet`].
    pub fn as_mutation_set(&self) -> &MutationSet<{ NullifierTree::DEPTH }, Word, Word> {
        &self.mutation_set
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Consumes self and returns the underlying [`MutationSet`].
    pub fn into_mutation_set(self) -> MutationSet<{ NullifierTree::DEPTH }, Word, Word> {
        self.mutation_set
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::NullifierTree;
    use crate::{NullifierTreeError, Word, block::BlockNumber, note::Nullifier};

    #[test]
    fn leaf_value_encoding() {
        let block_num = 123;
        let nullifier_value = NullifierTree::block_num_to_leaf_value(block_num.into());
        assert_eq!(nullifier_value, Word::from([block_num, 0, 0, 0u32]));
    }

    #[test]
    fn leaf_value_decoding() {
        let block_num = 123;
        let nullifier_value = Word::from([block_num, 0, 0, 0u32]);
        let decoded_block_num = NullifierTree::leaf_value_to_block_num(nullifier_value);

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
}
