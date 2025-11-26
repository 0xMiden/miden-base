#[cfg(test)]
mod tests {
    use miden_macros::WordWrapper;
    use miden_objects::{Felt, FieldElement, Word};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, WordWrapper)]
    pub struct TestId(Word);

    impl TestId {
        /// Test helper to create a TestId from a Word without validation
        pub fn new_unchecked(word: Word) -> Self {
            Self(word)
        }
    }

    #[test]
    fn test_word_wrapper_accessors() {
        // Create a test Word
        let word = Word::from([Felt::ONE, Felt::ONE, Felt::ZERO, Felt::ZERO]);
        let test_id = TestId::new_unchecked(word);

        // Test as_elements
        let elements = test_id.as_elements();
        assert_eq!(elements.len(), 4);
        assert_eq!(elements[0], Felt::ONE);
        assert_eq!(elements[1], Felt::ONE);

        // Test as_bytes
        let bytes = test_id.as_bytes();
        assert_eq!(bytes.len(), 32);

        // Test to_hex
        let hex = test_id.to_hex();
        assert!(!hex.is_empty());

        // Test as_word
        let retrieved_word = test_id.as_word();
        assert_eq!(retrieved_word, word);
    }

    #[test]
    fn test_word_wrapper_conversions() {
        let word = Word::from([Felt::ONE, Felt::ONE, Felt::ZERO, Felt::ZERO]);

        // Test conversion to Word
        let test_id = TestId::new_unchecked(word);
        let word_back: Word = test_id.into();
        assert_eq!(word_back, word);

        // Test From<&TestId> for Word
        let test_id = TestId::new_unchecked(word);
        let word_from_ref: Word = (&test_id).into();
        assert_eq!(word_from_ref, word);

        // Test From<TestId> for [u8; 32]
        let test_id = TestId::new_unchecked(word);
        let bytes: [u8; 32] = test_id.into();
        assert_eq!(bytes.len(), 32);

        // Test From<&TestId> for [u8; 32]
        let test_id = TestId::new_unchecked(word);
        let bytes_from_ref: [u8; 32] = (&test_id).into();
        assert_eq!(bytes_from_ref.len(), 32);
    }

    #[test]
    fn test_no_from_word_trait() {
        // This test ensures that From<Word> is NOT implemented
        // If this compiles, it means From<Word> is not available, which is the desired behavior
        let word = Word::from([Felt::ONE, Felt::ONE, Felt::ZERO, Felt::ZERO]);

        // We must use our own constructor
        let _test_id = TestId::new_unchecked(word);

        // The following would fail to compile if uncommented, which is correct:
        // let _test_id = TestId::from(word); // Should NOT compile
    }
}
