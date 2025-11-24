#[cfg(test)]
mod tests {
    use miden_macros::WordWrapper;
    use miden_objects::{Felt, FieldElement, Word};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, WordWrapper)]
    pub struct TestId(Word);

    #[test]
    fn test_word_wrapper_accessors() {
        // Create a test Word
        let word = Word::from([Felt::ONE, Felt::ONE, Felt::ZERO, Felt::ZERO]);
        let test_id = TestId::from(word);

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

        // Test From<Word>
        let test_id = TestId::from(word);

        // Test From<TestId> for Word
        let word_back: Word = test_id.into();
        assert_eq!(word_back, word);

        // Test From<&TestId> for Word
        let test_id = TestId::from(word);
        let word_from_ref: Word = (&test_id).into();
        assert_eq!(word_from_ref, word);

        // Test From<TestId> for [u8; 32]
        let test_id = TestId::from(word);
        let bytes: [u8; 32] = test_id.into();
        assert_eq!(bytes.len(), 32);

        // Test From<&TestId> for [u8; 32]
        let test_id = TestId::from(word);
        let bytes_from_ref: [u8; 32] = (&test_id).into();
        assert_eq!(bytes_from_ref.len(), 32);
    }
}
