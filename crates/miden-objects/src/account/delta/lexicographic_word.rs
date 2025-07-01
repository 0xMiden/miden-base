use core::cmp::Ordering;

use crate::{Felt, Word};

/// A [`Word`] wrapper with lexicographic ordering.
///
/// This is a wrapper around any [`Word`] convertible type that overrides the equality and ordering
/// implementations with a lexigographic one based on the wrapped type's [`Word`] representation.
#[derive(Debug, Clone, Copy)]
pub struct LexicographicWord<T: Into<Word> = Word>(T);

impl<T: Into<Word>> LexicographicWord<T> {
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

impl From<Word> for LexicographicWord {
    fn from(word: Word) -> Self {
        Self(word)
    }
}

impl<T: Into<Word>> From<LexicographicWord<T>> for Word {
    fn from(key: LexicographicWord<T>) -> Self {
        key.0.into()
    }
}

impl<T: Into<Word> + Copy> PartialEq for LexicographicWord<T> {
    fn eq(&self, other: &Self) -> bool {
        let self_word: Word = self.0.into();
        let other_word: Word = other.0.into();
        self_word == other_word
    }
}

impl<T: Into<Word> + Copy> Eq for LexicographicWord<T> {}

impl<T: Into<Word> + Copy> PartialOrd for LexicographicWord<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Into<Word> + Copy> Ord for LexicographicWord<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_word: Word = self.0.into();
        let other_word: Word = other.0.into();

        for (felt0, felt1) in self_word
            .iter()
            .rev()
            .map(Felt::as_int)
            .zip(other_word.iter().rev().map(Felt::as_int))
        {
            let ordering = felt0.cmp(&felt1);
            if let Ordering::Less | Ordering::Greater = ordering {
                return ordering;
            }
        }

        Ordering::Equal
    }
}
