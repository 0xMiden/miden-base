use core::cmp::Ordering;

use crate::{Felt, Word};

/// The key in a `LinkMap`.
///
/// This is a wrapper around a type that can be converted to [`Word`] and overrides the equality
/// and ordering implementations by implementing the link map ordering on the wrapped type's
/// [`Word`] representation.
#[derive(Debug, Clone, Copy)]
pub struct LinkMapKey<T: Into<Word> = Word>(T);

impl<T: Into<Word>> LinkMapKey<T> {
    /// Wraps the provided value into a new [`LinkMapKey`].
    pub fn new(inner: T) -> Self {
        Self(inner)
    }

    /// Returns a reference to the inner value.
    pub fn inner(&self) -> &T {
        &self.0
    }

    /// Consumes self and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl From<Word> for LinkMapKey {
    fn from(word: Word) -> Self {
        Self(word)
    }
}

impl<T: Into<Word>> From<LinkMapKey<T>> for Word {
    fn from(key: LinkMapKey<T>) -> Self {
        key.0.into()
    }
}

impl<T: Into<Word> + Copy> PartialEq for LinkMapKey<T> {
    fn eq(&self, other: &Self) -> bool {
        let self_word: Word = self.0.into();
        let other_word: Word = other.0.into();
        self_word == other_word
    }
}

impl<T: Into<Word> + Copy> Eq for LinkMapKey<T> {}

impl<T: Into<Word> + Copy> PartialOrd for LinkMapKey<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Into<Word> + Copy> Ord for LinkMapKey<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_word: Word = self.0.into();
        let other_word: Word = other.0.into();

        self_word
            .iter()
            .rev()
            .map(Felt::as_int)
            .zip(other_word.iter().rev().map(Felt::as_int))
            .fold(Ordering::Equal, |ord, (felt0, felt1)| match ord {
                Ordering::Equal => felt0.cmp(&felt1),
                _ => ord,
            })
    }
}
