use alloc::vec::Vec;

use crate::errors::NoteError;
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{Felt, Hasher, MAX_STORAGE_VALUE_PER_NOTE, WORD_SIZE, Word, ZERO};

// NOTE STORAGE
// ================================================================================================

/// A container for note storage.
///
/// A note can be associated with up to 128 storage items. Each value is represented by a single
/// field element. Thus, note storage items can contain up to ~1 KB of data.
///
/// All storage items associated with a note can be reduced to a single commitment which is
/// computed by first padding the values with ZEROs to the next multiple of 8, and then by computing
/// a sequential hash of the resulting elements.
#[derive(Clone, Debug)]
pub struct NoteStorage {
    items: Vec<Felt>,
    commitment: Word,
}

impl NoteStorage {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns [NoteStorage] instantiated from the provided values.
    ///
    /// # Errors
    /// Returns an error if the number of provided storage is greater than 128.
    pub fn new(values: Vec<Felt>) -> Result<Self, NoteError> {
        if values.len() > MAX_STORAGE_VALUE_PER_NOTE {
            return Err(NoteError::TooManyStorageValues(values.len()));
        }

        Ok(pad_and_build(values))
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a commitment to these storage items.
    pub fn commitment(&self) -> Word {
        self.commitment
    }

    /// Returns the number of storage items.
    ///
    /// The returned value is guaranteed to be smaller than or equal to 128.
    pub fn num_values(&self) -> u8 {
        const _: () = assert!(MAX_STORAGE_VALUE_PER_NOTE <= u8::MAX as usize);
        debug_assert!(
            self.items.len() < MAX_STORAGE_VALUE_PER_NOTE,
            "The constructor should have checked the number of storage items"
        );
        self.items.len() as u8
    }

    /// Returns a reference to the storage items.
    pub fn items(&self) -> &[Felt] {
        &self.items
    }

    /// Returns the note's storage items formatted to be used with the advice map.
    ///
    /// The format is `STORAGE || PADDING`, where:
    ///
    /// Where:
    /// - STORAGE is the variable storage item for the note
    /// - PADDING is the optional padding to align the data with a 2WORD boundary
    pub fn format_for_advice(&self) -> Vec<Felt> {
        pad_storage(&self.items)
    }
}

impl Default for NoteStorage {
    fn default() -> Self {
        pad_and_build(vec![])
    }
}

impl PartialEq for NoteStorage {
    fn eq(&self, other: &Self) -> bool {
        let NoteStorage { items, commitment: _ } = self;
        items == &other.items
    }
}

impl Eq for NoteStorage {}

// CONVERSION
// ================================================================================================

impl From<NoteStorage> for Vec<Felt> {
    fn from(value: NoteStorage) -> Self {
        value.items
    }
}

impl TryFrom<Vec<Felt>> for NoteStorage {
    type Error = NoteError;

    fn try_from(value: Vec<Felt>) -> Result<Self, Self::Error> {
        NoteStorage::new(value)
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Returns a vector built from the provided storage items and padded to the next multiple of
/// 8.
fn pad_storage(storage_items: &[Felt]) -> Vec<Felt> {
    const BLOCK_SIZE: usize = WORD_SIZE * 2;

    let padded_len = storage_items.len().next_multiple_of(BLOCK_SIZE);
    let mut padded_storage = Vec::with_capacity(padded_len);
    padded_storage.extend(storage_items.iter());
    padded_storage.resize(padded_len, ZERO);

    padded_storage
}

/// Pad `items` and returns a new `NoteStorage`.
fn pad_and_build(items: Vec<Felt>) -> NoteStorage {
    let commitment = {
        let padded_values = pad_storage(&items);
        Hasher::hash_elements(&padded_values)
    };

    NoteStorage { items, commitment }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteStorage {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let NoteStorage { items, commitment: _commitment } = self;
        target.write_u8(items.len().try_into().expect("storage len is not a u8 value"));
        target.write_many(items);
    }
}

impl Deserializable for NoteStorage {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_values = source.read_u8()? as usize;
        let values = source.read_many::<Felt>(num_values)?;
        Self::new(values).map_err(|v| DeserializationError::InvalidValue(format!("{v}")))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_crypto::utils::Deserializable;

    use super::{Felt, NoteStorage, Serializable};

    #[test]
    fn test_storage_value_ordering() {
        // values are provided in reverse stack order
        let storage_items = vec![Felt::new(1), Felt::new(2), Felt::new(3)];
        // we expect the storage items to be padded to length 16 and to remain in reverse stack
        // order.
        let expected_ordering = vec![Felt::new(1), Felt::new(2), Felt::new(3)];

        let note_storage_items =
            NoteStorage::new(storage_items).expect("note created should succeed");
        assert_eq!(&expected_ordering, &note_storage_items.items);
    }

    #[test]
    fn test_storage_value_serialization() {
        let storage_items = vec![Felt::new(1), Felt::new(2), Felt::new(3)];
        let note_storage_items = NoteStorage::new(storage_items).unwrap();

        let bytes = note_storage_items.to_bytes();
        let parsed_note_storage_items = NoteStorage::read_from_bytes(&bytes).unwrap();
        assert_eq!(note_storage_items, parsed_note_storage_items);
    }
}
