use alloc::vec::Vec;

use crate::utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable};

/// Writes a vector of serializable items into the target writer, prefixed by its length as a
/// 16-bit unsigned integer.
///
/// # Panics
/// Panics if the length of the vector exceeds `u16::MAX` – this should never happen for note
/// collections because the constructor of both `InputNotes` and `OutputNotes` enforces this
/// invariant.
pub(crate) fn write_vec_with_len<T, W>(items: &[T], target: &mut W)
where
    T: Serializable,
    W: ByteWriter,
{
    assert!(items.len() <= u16::MAX as usize);
    target.write_u16(items.len() as u16);
    target.write_many(items);
}

/// Reads a vector that was written with [`write_vec_with_len`]. The vector is expected to be
/// prefixed by a 16-bit length.
pub(crate) fn read_vec_with_len<T, R>(source: &mut R) -> Result<Vec<T>, DeserializationError>
where
    T: Deserializable,
    R: ByteReader,
{
    let num_items = source.read_u16()?;
    source.read_many::<T>(num_items.into())
}

// ------------------------------------------------------------------------------------------------
// NOTE COLLECTION TRAIT
// ------------------------------------------------------------------------------------------------

/// Common behaviour for collections of notes used as transaction inputs / outputs.
///
/// The trait is intentionally minimal: a single required `notes()` accessor, with all helper
/// methods (`num_notes`, `is_empty`, `get_note`, `iter`) provided via default implementations.
///
/// This allows different note collection structs to expose consistent APIs without having to
/// repeat the boiler-plate code, while still giving each struct the freedom to provide additional
/// inherent methods (like `num_notes` returning a specific integer type).
pub trait NoteCollection {
    /// The concrete note type held by the collection.
    type Note;

    /// Borrow the underlying slice of notes.
    fn notes(&self) -> &[Self::Note];

    /// Returns total number of notes (as `usize`).
    #[inline]
    fn num_notes_usize(&self) -> usize {
        self.notes().len()
    }

    /// Returns `true` if the collection is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.notes().is_empty()
    }

    /// Returns a reference to the note at the given index.
    #[inline]
    fn get_note(&self, idx: usize) -> &Self::Note {
        &self.notes()[idx]
    }

    /// Returns an iterator over the notes.
    #[inline]
    fn iter(&self) -> core::slice::Iter<'_, Self::Note> {
        self.notes().iter()
    }
}

// ------------------------------------------------------------------------------------------------
// (Previously there was a macro for generating helpers; now superseded by `NoteCollection` trait)
// ------------------------------------------------------------------------------------------------