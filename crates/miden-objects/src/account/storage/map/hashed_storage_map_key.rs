use core::fmt;

use miden_crypto::Word;

use crate::Felt;

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct HashedStorageMapKey(Word);

impl HashedStorageMapKey {
    /// Creates a new [`HashedStorageMapKey`] from the given [`Word`] **without performing
    /// validation**.
    ///
    /// ## Warning
    ///
    /// This function **does not check** whether the provided `Word` represents a valid
    /// fungible or non-fungible asset key.
    pub fn new_unchecked(value: Word) -> Self {
        Self(value)
    }

    pub fn inner(&self) -> Word {
        self.0
    }

    /// Returns the leaf index of a map key.
    pub fn hashed_map_key_to_leaf_index(&self) -> Felt {
        // The third element in an SMT key is the index.
        self.0[3]
    }
}

impl fmt::Display for HashedStorageMapKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
