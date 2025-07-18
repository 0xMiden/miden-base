use miden_crypto::merkle::{MerkleError, MutationSet, Smt, SmtLeaf};
use vm_processor::SMT_DEPTH;

use crate::{
    Felt, Word,
    account::{AccountId, AccountIdPrefix},
    block::AccountWitness,
    errors::AccountTreeError,
};

// ACCOUNT TREE
// ================================================================================================

/// The sparse merkle tree of all accounts in the blockchain.
///
/// The key is the [`AccountId`] while the value is the current state commitment of the account,
/// i.e. [`Account::commitment`](crate::account::Account::commitment). If the account is new, then
/// the commitment is the [`EMPTY_WORD`](crate::EMPTY_WORD).
///
/// Each account ID occupies exactly one leaf in the tree, which is identified by its
/// [`AccountId::prefix`]. In other words, account ID prefixes are unique in the blockchain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTree {
    smt: Smt,
}

impl AccountTree {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The depth of the account tree.
    pub const DEPTH: u8 = SMT_DEPTH;

    /// The index of the account ID suffix in the SMT key.
    pub(super) const KEY_SUFFIX_IDX: usize = 2;
    /// The index of the account ID prefix in the SMT key.
    pub(super) const KEY_PREFIX_IDX: usize = 3;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new, empty account tree.
    pub fn new() -> Self {
        AccountTree { smt: Smt::new() }
    }

    /// Returns a new [`Smt`] instantiated with the provided entries.
    ///
    /// If the `concurrent` feature of `miden-crypto` is enabled, this function uses a parallel
    /// implementation to process the entries efficiently, otherwise it defaults to the
    /// sequential implementation.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided entries contain multiple commitments for the same account ID.
    /// - multiple account IDs share the same prefix.
    pub fn with_entries<I>(
        entries: impl IntoIterator<Item = (AccountId, Word), IntoIter = I>,
    ) -> Result<Self, AccountTreeError>
    where
        I: ExactSizeIterator<Item = (AccountId, Word)>,
    {
        let entries = entries.into_iter();
        let num_accounts = entries.len();

        let smt = Smt::with_entries(
            entries.map(|(id, commitment)| (Self::id_to_smt_key(id), commitment)),
        )
        .map_err(|err| {
            let MerkleError::DuplicateValuesForIndex(leaf_idx) = err else {
                unreachable!("the only error returned by Smt::with_entries is of this type");
            };

            // SAFETY: Since we only inserted account IDs into the SMT, it is guaranteed that
            // the leaf_idx is a valid Felt as well as a valid account ID prefix.
            AccountTreeError::DuplicateStateCommitments {
                prefix: AccountIdPrefix::new_unchecked(
                    Felt::try_from(leaf_idx).expect("leaf index should be a valid felt"),
                ),
            }
        })?;

        // If the number of leaves in the SMT is smaller than the number of accounts that were
        // passed in, it means that at least one account ID pair ended up in the same leaf. If this
        // is the case, we iterate the SMT entries to find the duplicated account ID prefix.
        if smt.num_leaves() < num_accounts {
            for (leaf_idx, leaf) in smt.leaves() {
                if leaf.num_entries() >= 2 {
                    // SAFETY: Since we only inserted account IDs into the SMT, it is guaranteed
                    // that the leaf_idx is a valid Felt as well as a valid
                    // account ID prefix.
                    return Err(AccountTreeError::DuplicateIdPrefix {
                        duplicate_prefix: AccountIdPrefix::new_unchecked(
                            Felt::try_from(leaf_idx.value())
                                .expect("leaf index should be a valid felt"),
                        ),
                    });
                }
            }
        }

        Ok(AccountTree { smt })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns an opening of the leaf associated with the `account_id`. This is a proof of the
    /// current state commitment of the given account ID.
    ///
    /// Conceptually, an opening is a Merkle path to the leaf, as well as the leaf itself.
    pub fn open(&self, account_id: AccountId) -> AccountWitness {
        let key = Self::id_to_smt_key(account_id);
        let proof = self.smt.open(&key);

        AccountWitness::from_smt_proof(account_id, proof)
    }

    /// Returns the current state commitment of the given account ID.
    pub fn get(&self, account_id: AccountId) -> Word {
        let key = Self::id_to_smt_key(account_id);
        self.smt.get_value(&key)
    }

    /// Returns the root of the tree.
    pub fn root(&self) -> Word {
        self.smt.root()
    }

    /// Returns true if the tree contains a leaf for the given account ID.
    pub fn has_leaf_for_account(&self, account_id: AccountId) -> bool {
        let key = Self::id_to_smt_key(account_id);
        let is_empty = matches!(self.smt.get_leaf(&key), SmtLeaf::Empty(_));
        !is_empty
    }

    /// Returns the number of account IDs in this tree.
    pub fn num_accounts(&self) -> usize {
        // Because each ID's prefix is unique in the tree and occupies a single leaf, the number of
        // IDs in the tree is equivalent to the number of leaves in the tree.
        self.smt.num_leaves()
    }

    /// Returns an iterator over the account ID state commitment pairs in the tree.
    pub fn account_commitments(&self) -> impl Iterator<Item = (AccountId, Word)> {
        self.smt.leaves().map(|(_leaf_idx, leaf)| {
            // SAFETY: By construction no Multiple variant is ever present in the tree.
            // The Empty variant is not returned by Smt::leaves, because it only returns leaves that
            // are actually present.
            let SmtLeaf::Single((key, commitment)) = leaf else {
                unreachable!("empty and multiple variant should never be encountered")
            };

            (
                // SAFETY: By construction, the tree only contains valid IDs.
                AccountId::try_from([key[Self::KEY_PREFIX_IDX], key[Self::KEY_SUFFIX_IDX]])
                    .expect("account tree should only contain valid IDs"),
                *commitment,
            )
        })
    }

    /// Computes the necessary changes to insert the specified (account ID, state commitment) pairs
    /// into this tree, allowing for validation before applying those changes.
    ///
    /// [`Self::apply_mutations`] can be used in order to commit these changes to the tree.
    ///
    /// If the `concurrent` feature of `miden-crypto` is enabled, this function uses a parallel
    /// implementation to compute the mutations, otherwise it defaults to the sequential
    /// implementation.
    ///
    /// This is a thin wrapper around [`Smt::compute_mutations`]. See its documentation for more
    /// details.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - an insertion of an account ID would violate the uniqueness of account ID prefixes in the
    ///   tree.
    pub fn compute_mutations(
        &self,
        account_commitments: impl IntoIterator<Item = (AccountId, Word)>,
    ) -> Result<AccountMutationSet, AccountTreeError> {
        let mutation_set = self.smt.compute_mutations(
            account_commitments
                .into_iter()
                .map(|(id, commitment)| (Self::id_to_smt_key(id), commitment)),
        );

        for id_key in mutation_set.new_pairs().keys() {
            // Check if the insertion would be valid.
            match self.smt.get_leaf(id_key) {
                // Inserting into an empty leaf is valid.
                SmtLeaf::Empty(_) => (),
                SmtLeaf::Single((existing_key, _)) => {
                    // If the key matches the existing one, then we're updating the leaf, which is
                    // valid. If it does not match, then we would insert a duplicate.
                    if existing_key != *id_key {
                        return Err(AccountTreeError::DuplicateIdPrefix {
                            duplicate_prefix: Self::smt_key_to_id(*id_key).prefix(),
                        });
                    }
                },
                SmtLeaf::Multiple(_) => {
                    unreachable!(
                        "account tree should never contain duplicate ID prefixes and therefore never a multiple leaf"
                    )
                },
            }
        }

        Ok(AccountMutationSet::new(mutation_set))
    }

    /// Checks if the given account ID prefix exists in the tree.
    pub fn exists(&mut self, account_id: AccountId) -> bool {
        let key = Self::id_to_smt_key(account_id);
        !self.smt.get_leaf(&key).is_empty()
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Inserts the state commitment for the given account ID, returning the previous state
    /// commitment associated with that ID.
    ///
    /// This also recomputes all hashes between the leaf (associated with the key) and the root,
    /// updating the root itself.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the prefix of the account ID already exists in the tree.
    pub fn insert(
        &mut self,
        account_id: AccountId,
        state_commitment: Word,
    ) -> Result<Word, AccountTreeError> {
        let key = Self::id_to_smt_key(account_id);
        let prev_value = self.smt.insert(key, state_commitment);

        // If the leaf of the account ID now has two or more entries, we've inserted a duplicate
        // prefix.
        if self.smt.get_leaf(&key).num_entries() >= 2 {
            return Err(AccountTreeError::DuplicateIdPrefix {
                duplicate_prefix: account_id.prefix(),
            });
        }

        Ok(prev_value)
    }

    /// Applies the prospective mutations computed with [`Self::compute_mutations`] to this tree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `mutations` was computed on a tree with a different root than this one.
    pub fn apply_mutations(
        &mut self,
        mutations: AccountMutationSet,
    ) -> Result<(), AccountTreeError> {
        self.smt
            .apply_mutations(mutations.into_mutation_set())
            .map_err(AccountTreeError::ApplyMutations)
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Returns the SMT key of the given account ID.
    pub(super) fn id_to_smt_key(account_id: AccountId) -> Word {
        // We construct this in such a way that we're forced to use the constants, so that when
        // they're updated, the other usages of the constants are also updated.
        let mut key = Word::empty();
        key[Self::KEY_SUFFIX_IDX] = account_id.suffix();
        key[Self::KEY_PREFIX_IDX] = account_id.prefix().as_felt();

        key
    }

    /// Returns the [`AccountId`] recovered from the given SMT key.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the key is not a valid account ID. This should not happen when used on keys from (partial)
    ///   account tree.
    pub(super) fn smt_key_to_id(key: Word) -> AccountId {
        AccountId::try_from([key[Self::KEY_PREFIX_IDX], key[Self::KEY_SUFFIX_IDX]])
            .expect("account tree should only contain valid IDs")
    }
}

impl Default for AccountTree {
    fn default() -> Self {
        Self::new()
    }
}

// ACCOUNT MUTATION SET
// ================================================================================================

/// A newtype wrapper around a [`MutationSet`] for use in the [`AccountTree`].
///
/// It guarantees that applying the contained mutations will result in an account tree with unique
/// account ID prefixes.
///
/// It is returned by and used in methods on the [`AccountTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountMutationSet {
    mutation_set: MutationSet<{ AccountTree::DEPTH }, Word, Word>,
}

impl AccountMutationSet {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`AccountMutationSet`] from the provided raw mutation set.
    fn new(mutation_set: MutationSet<{ AccountTree::DEPTH }, Word, Word>) -> Self {
        Self { mutation_set }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the underlying [`MutationSet`].
    pub fn as_mutation_set(&self) -> &MutationSet<{ AccountTree::DEPTH }, Word, Word> {
        &self.mutation_set
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Consumes self and returns the underlying [`MutationSet`].
    pub fn into_mutation_set(self) -> MutationSet<{ AccountTree::DEPTH }, Word, Word> {
        self.mutation_set
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
pub(super) mod tests {
    use std::vec::Vec;

    use assert_matches::assert_matches;

    use super::*;
    use crate::{
        account::{AccountStorageMode, AccountType},
        testing::account_id::{AccountIdBuilder, account_id},
    };

    pub(crate) fn setup_duplicate_prefix_ids() -> [(AccountId, Word); 2] {
        let id0 = AccountId::try_from(account_id(
            AccountType::FungibleFaucet,
            AccountStorageMode::Public,
            0xaabb_ccdd,
        ))
        .unwrap();
        let id1 = AccountId::try_from(account_id(
            AccountType::FungibleFaucet,
            AccountStorageMode::Public,
            0xaabb_ccff,
        ))
        .unwrap();
        assert_eq!(id0.prefix(), id1.prefix(), "test requires that these ids have the same prefix");

        let commitment0 = Word::from([0, 0, 0, 42u32]);
        let commitment1 = Word::from([0, 0, 0, 24u32]);

        assert_eq!(id0.prefix(), id1.prefix(), "test requires that these ids have the same prefix");
        [(id0, commitment0), (id1, commitment1)]
    }

    #[test]
    fn insert_fails_on_duplicate_prefix() {
        let mut tree = AccountTree::new();
        let [(id0, commitment0), (id1, commitment1)] = setup_duplicate_prefix_ids();

        tree.insert(id0, commitment0).unwrap();
        assert_eq!(tree.get(id0), commitment0);

        let err = tree.insert(id1, commitment1).unwrap_err();

        assert_matches!(err, AccountTreeError::DuplicateIdPrefix {
          duplicate_prefix
        } if duplicate_prefix == id0.prefix());
    }

    #[test]
    fn with_entries_fails_on_duplicate_prefix() {
        let entries = setup_duplicate_prefix_ids();

        let err = AccountTree::with_entries(entries.iter().copied()).unwrap_err();

        assert_matches!(err, AccountTreeError::DuplicateIdPrefix {
          duplicate_prefix
        } if duplicate_prefix == entries[0].0.prefix());
    }

    #[test]
    fn insert_succeeds_on_multiple_updates() {
        let mut tree = AccountTree::new();
        let [(id0, commitment0), (_, commitment1)] = setup_duplicate_prefix_ids();

        tree.insert(id0, commitment0).unwrap();
        tree.insert(id0, commitment1).unwrap();
        assert_eq!(tree.get(id0), commitment1);
    }

    #[test]
    fn apply_mutations() {
        let id0 = AccountIdBuilder::new().build_with_seed([5; 32]);
        let id1 = AccountIdBuilder::new().build_with_seed([6; 32]);
        let id2 = AccountIdBuilder::new().build_with_seed([7; 32]);

        let digest0 = Word::from([0, 0, 0, 1u32]);
        let digest1 = Word::from([0, 0, 0, 2u32]);
        let digest2 = Word::from([0, 0, 0, 3u32]);
        let digest3 = Word::from([0, 0, 0, 4u32]);

        let mut tree = AccountTree::with_entries([(id0, digest0), (id1, digest1)]).unwrap();

        let mutations = tree
            .compute_mutations([(id0, digest1), (id1, digest2), (id2, digest3)])
            .unwrap();

        tree.apply_mutations(mutations).unwrap();

        assert_eq!(tree.num_accounts(), 3);
        assert_eq!(tree.get(id0), digest1);
        assert_eq!(tree.get(id1), digest2);
        assert_eq!(tree.get(id2), digest3);
    }

    #[test]
    fn duplicates_in_compute_mutations() {
        let [pair0, pair1] = setup_duplicate_prefix_ids();
        let id2 = AccountIdBuilder::new().build_with_seed([5; 32]);
        let commitment2 = Word::from([0, 0, 0, 99u32]);

        let tree = AccountTree::with_entries([pair0, (id2, commitment2)]).unwrap();

        let err = tree.compute_mutations([pair1]).unwrap_err();

        assert_matches!(err, AccountTreeError::DuplicateIdPrefix {
          duplicate_prefix
        } if duplicate_prefix == pair1.0.prefix());
    }

    #[test]
    fn account_commitments() {
        let id0 = AccountIdBuilder::new().build_with_seed([5; 32]);
        let id1 = AccountIdBuilder::new().build_with_seed([6; 32]);
        let id2 = AccountIdBuilder::new().build_with_seed([7; 32]);

        let digest0 = Word::from([0, 0, 0, 1u32]);
        let digest1 = Word::from([0, 0, 0, 2u32]);
        let digest2 = Word::from([0, 0, 0, 3u32]);
        let empty_digest = Word::empty();

        let mut tree =
            AccountTree::with_entries([(id0, digest0), (id1, digest1), (id2, digest2)]).unwrap();

        // remove id2
        tree.insert(id2, empty_digest).unwrap();

        assert_eq!(tree.num_accounts(), 2);

        let accounts: Vec<_> = tree.account_commitments().collect();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.contains(&(id0, digest0)));
        assert!(accounts.contains(&(id1, digest1)));
    }

    #[test]
    fn account_witness() {
        let id0 = AccountIdBuilder::new().build_with_seed([5; 32]);
        let id1 = AccountIdBuilder::new().build_with_seed([6; 32]);

        let digest0 = Word::from([0, 0, 0, 1u32]);
        let digest1 = Word::from([0, 0, 0, 2u32]);

        let tree = AccountTree::with_entries([(id0, digest0), (id1, digest1)]).unwrap();

        assert_eq!(tree.num_accounts(), 2);

        for id in [id0, id1] {
            let (control_path, control_leaf) =
                tree.smt.open(&AccountTree::id_to_smt_key(id)).into_parts();
            let witness = tree.open(id);

            assert_eq!(witness.leaf(), control_leaf);
            assert_eq!(witness.path(), &control_path);
        }
    }

    #[test]
    fn has_account_leaf() {
        // Create a tree with a single account.
        let [pair0, pair1] = setup_duplicate_prefix_ids();
        let tree = AccountTree::with_entries([(pair0.0, pair0.1)]).unwrap();
        assert_eq!(tree.num_accounts(), 1);

        // Validate the account leaf exists.
        assert!(tree.has_leaf_for_account(pair0.0));

        // Validate the account leaf with same prefix exists.
        assert!(tree.has_leaf_for_account(pair1.0));

        // Validate the account leaf does not exist.
        let id1 = AccountIdBuilder::new().build_with_seed([7; 32]);
        assert!(!tree.has_leaf_for_account(id1));
    }
}
