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
// MACROS
// ------------------------------------------------------------------------------------------------

/// Implements common getter/helper methods (`is_empty`, `get_note`, and `iter`) for note
/// collection structs that internally store a `Vec` named `notes`.
///
/// The macro supports both generic and non-generic structs:
///
/// ```ignore
/// // Non-generic collection
/// impl_note_collection_getters!(OutputNotes, OutputNote);
///
/// // Generic collection
/// impl_note_collection_getters!(InputNotes<T>);
/// ```
#[macro_export]
macro_rules! impl_note_collection_getters {
    // Non-generic struct – the item type must be supplied explicitly so the macro can use it in
    // method signatures.
    ($struct:ident, $item:ty) => {
        impl $struct {
            #[inline]
            pub fn is_empty(&self) -> bool {
                self.notes.is_empty()
            }

            #[inline]
            pub fn get_note(&self, idx: usize) -> &$item {
                &self.notes[idx]
            }

            #[inline]
            pub fn iter(&self) -> core::slice::Iter<'_, $item> {
                self.notes.iter()
            }
        }
    };

    // Generic struct – the item type is the generic parameter itself.
    ($struct:ident < $generic:ident >) => {
        impl<$generic> $struct<$generic> {
            #[inline]
            pub fn is_empty(&self) -> bool {
                self.notes.is_empty()
            }

            #[inline]
            pub fn get_note(&self, idx: usize) -> &$generic {
                &self.notes[idx]
            }
            #[inline]
            pub fn iter(&self) -> core::slice::Iter<'_, $generic> {
                self.notes.iter()
            }
        }
    };
}