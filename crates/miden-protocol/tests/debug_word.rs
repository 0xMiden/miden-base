use miden_protocol::{Felt, Word, EMPTY_WORD, ZERO};

#[test]
fn test_word_empty_vs_default() {
    let empty = Word::empty();
    let default_word = Word::default();

    println!("Word::empty() = {:?}", empty);
    println!("Word::default() = {:?}", default_word);
    println!("EMPTY_WORD = {:?}", EMPTY_WORD);
    println!("Are Word::empty() and Word::default() equal? {}", empty == default_word);
    println!("Are Word::empty() and EMPTY_WORD equal? {}", empty == EMPTY_WORD);
    println!("Are Word::default() and EMPTY_WORD equal? {}", default_word == EMPTY_WORD);

    // Check individual elements
    for i in 0..4 {
        println!("empty[{}] = {:?}, default[{}] = {:?}, EMPTY_WORD[{}] = {:?}",
                 i, empty[i], i, default_word[i], i, EMPTY_WORD[i]);
    }

    assert_eq!(empty, default_word, "Word::empty() should equal Word::default()");
    assert_eq!(empty, EMPTY_WORD, "Word::empty() should equal EMPTY_WORD");
}
