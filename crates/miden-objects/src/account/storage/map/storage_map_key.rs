use core::fmt;

use miden_crypto::Word;
use miden_crypto::word::LexicographicWord;

use super::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Felt,
    HashedStorageMapKey,
    Serializable,
};
use crate::Hasher;
use crate::asset::AssetVaultKey;

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct StorageMapKey(Word);

impl StorageMapKey {
    /// Creates a new [`StorageMapKey`] from the given [`Word`] **without performing validation**.
    ///
    /// ## Warning
    ///
    /// This function **does not check** whether the provided `Word` represents a valid
    /// fungible or non-fungible asset key.
    pub const fn new_unchecked(value: Word) -> Self {
        Self(value)
    }

    pub fn inner(&self) -> Word {
        self.0
    }

    /// Hashes the given key to get the key of the SMT.
    pub fn hash(&self) -> HashedStorageMapKey {
        HashedStorageMapKey::new_unchecked(Hasher::hash_elements(self.0.as_elements()))
    }

    pub fn as_elements(&self) -> &[Felt] {
        self.0.as_elements()
    }
}

impl fmt::Display for StorageMapKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for StorageMapKey {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.0.write_into(target);
    }

    fn get_size_hint(&self) -> usize {
        self.0.get_size_hint()
    }
}

impl Deserializable for StorageMapKey {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let word = source.read()?;
        Ok(Self::new_unchecked(word))
    }
}

impl From<Word> for StorageMapKey {
    fn from(value: Word) -> Self {
        Self::new_unchecked(value)
    }
}

impl From<StorageMapKey> for Word {
    fn from(value: StorageMapKey) -> Self {
        value.0
    }
}

impl From<StorageMapKey> for LexicographicWord {
    fn from(value: StorageMapKey) -> Self {
        LexicographicWord::from(value.0)
    }
}

impl From<AssetVaultKey> for StorageMapKey {
    fn from(vault_key: AssetVaultKey) -> Self {
        let vault_key_word: Word = vault_key.into();
        StorageMapKey::from(vault_key_word)
    }
}

impl From<[Felt; 4]> for StorageMapKey {
    fn from(value: [Felt; 4]) -> Self {
        StorageMapKey::new_unchecked(Word::from(value))
    }
}
