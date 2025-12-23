use miden_protocol::{Felt, Word, EMPTY_WORD};

#[test]
fn test_word_is_empty() {
    let empty = Word::empty();
    let default_word = Word::default();
    let non_empty = Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);

    println!("empty.is_empty() = {}", empty.is_empty());
    println!("default_word.is_empty() = {}", default_word.is_empty());
    println!("EMPTY_WORD.is_empty() = {}", EMPTY_WORD.is_empty());
    println!("non_empty.is_empty() = {}", non_empty.is_empty());

    assert!(empty.is_empty(), "Word::empty() should be empty");
    assert!(default_word.is_empty(), "Word::default() should be empty");
    assert!(EMPTY_WORD.is_empty(), "EMPTY_WORD should be empty");
    assert!(!non_empty.is_empty(), "non-empty Word should not be empty");
}
