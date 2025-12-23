use miden_protocol::{Felt, Word, EMPTY_WORD};
use miden_core::EMPTY_WORD as CORE_EMPTY_WORD;

#[test]
fn test_empty_word_comparison() {
    let empty1 = Word::empty();
    let empty2 = Word::default();
    let empty3 = EMPTY_WORD;
    let empty4 = CORE_EMPTY_WORD;
    let zero_word = Word::from([Felt::new(0), Felt::new(0), Felt::new(0), Felt::new(0)]);

    println!("empty1 == EMPTY_WORD: {}", empty1 == EMPTY_WORD);
    println!("empty2 == EMPTY_WORD: {}", empty2 == EMPTY_WORD);
    println!("empty3 == EMPTY_WORD: {}", empty3 == EMPTY_WORD);
    println!("empty4 == EMPTY_WORD: {}", empty4 == EMPTY_WORD);
    println!("zero_word == EMPTY_WORD: {}", zero_word == EMPTY_WORD);

    println!("\nAre EMPTY_WORD and CORE_EMPTY_WORD the same? {}", EMPTY_WORD == CORE_EMPTY_WORD);
    println!("EMPTY_WORD address: {:p}", &EMPTY_WORD);
    println!("CORE_EMPTY_WORD address: {:p}", &CORE_EMPTY_WORD);

    assert!(empty1 == EMPTY_WORD);
    assert!(empty2 == EMPTY_WORD);
    assert!(zero_word == EMPTY_WORD);
}
