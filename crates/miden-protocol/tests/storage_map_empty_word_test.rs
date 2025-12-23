use miden_protocol::{
    account::{AccountStorageMode, AccountType, StorageMap, StorageMapDelta},
    Felt, Word, EMPTY_WORD,
};

#[test]
fn test_storage_map_insert_empty_word() {
    // Create a storage map with some initial entries
    let key1 = Word::from([Felt::new(1), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let key2 = Word::from([Felt::new(2), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let key3 = Word::from([Felt::new(3), Felt::new(0), Felt::new(0), Felt::new(0)]);

    let value1 = Word::from([Felt::new(100), Felt::new(200), Felt::new(300), Felt::new(400)]);
    let value2 = Word::from([Felt::new(500), Felt::new(600), Felt::new(700), Felt::new(800)]);
    let value3 = Word::from([Felt::new(900), Felt::new(1000), Felt::new(1100), Felt::new(1200)]);

    let mut map = StorageMap::with_entries(vec![
        (key1, value1),
        (key2, value2),
        (key3, value3),
    ].into_iter()).unwrap();

    println!("Initial map entries:");
    println!("  key1 -> {:?}", map.get(&key1));
    println!("  key2 -> {:?}", map.get(&key2));
    println!("  key3 -> {:?}", map.get(&key3));
    println!("  num_entries: {}", map.num_entries());

    // Now insert EMPTY_WORD for key2 (should remove it)
    let old_value = map.insert(key2, EMPTY_WORD).unwrap();
    println!("\nAfter inserting EMPTY_WORD for key2:");
    println!("  old_value: {:?}", old_value);
    println!("  key2 -> {:?}", map.get(&key2));
    println!("  num_entries: {}", map.num_entries());

    // Verify key2 now returns EMPTY_WORD
    let retrieved = map.get(&key2);
    println!("\nComparison:");
    println!("  retrieved == EMPTY_WORD: {}", retrieved == EMPTY_WORD);
    println!("  retrieved == Word::empty(): {}", retrieved == Word::empty());
    println!("  retrieved == Word::default(): {}", retrieved == Word::default());
    println!("  retrieved.is_empty(): {}", retrieved.is_empty());

    assert_eq!(retrieved, EMPTY_WORD, "After inserting EMPTY_WORD, get should return EMPTY_WORD");
    assert_eq!(retrieved, Word::empty(), "Retrieved value should equal Word::empty()");
    assert!(retrieved.is_empty(), "Retrieved value should be empty");

    // Other keys should still work
    assert_eq!(map.get(&key1), value1);
    assert_eq!(map.get(&key3), value3);
}

#[test]
fn test_storage_map_apply_delta_with_empty_word() {
    // Create a storage map with initial entries
    let key1 = Word::from([Felt::new(1), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let key2 = Word::from([Felt::new(2), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let key3 = Word::from([Felt::new(3), Felt::new(0), Felt::new(0), Felt::new(0)]);

    let value1 = Word::from([Felt::new(100), Felt::new(200), Felt::new(300), Felt::new(400)]);
    let value2 = Word::from([Felt::new(500), Felt::new(600), Felt::new(700), Felt::new(800)]);
    let value3 = Word::from([Felt::new(900), Felt::new(1000), Felt::new(1100), Felt::new(1200)]);

    let mut map = StorageMap::with_entries(vec![
        (key1, value1),
        (key2, value2),
        (key3, value3),
    ].into_iter()).unwrap();

    println!("Initial map entries:");
    println!("  key1 -> {:?}", map.get(&key1));
    println!("  key2 -> {:?}", map.get(&key2));
    println!("  key3 -> {:?}", map.get(&key3));
    println!("  num_entries: {}", map.num_entries());

    // Create a delta that clears key2 and updates key3
    let new_value3 = Word::from([Felt::new(111), Felt::new(222), Felt::new(333), Felt::new(444)]);
    let delta = StorageMapDelta::from_iters(
        vec![key2],  // cleared keys
        vec![(key3, new_value3)],  // updated keys
    );

    println!("\nDelta contents:");
    for (k, v) in delta.entries() {
        println!("  {:?} -> {:?} (is_empty: {})", k.inner(), v, v.is_empty());
    }

    // Apply the delta
    map.apply_delta(&delta).unwrap();

    println!("\nAfter applying delta:");
    println!("  key1 -> {:?}", map.get(&key1));
    println!("  key2 -> {:?} (should be empty)", map.get(&key2));
    println!("  key3 -> {:?}", map.get(&key3));
    println!("  num_entries: {}", map.num_entries());

    // Verify the results
    assert_eq!(map.get(&key1), value1, "key1 should be unchanged");

    let key2_value = map.get(&key2);
    println!("\nDetailed key2 check:");
    println!("  key2_value: {:?}", key2_value);
    println!("  EMPTY_WORD: {:?}", EMPTY_WORD);
    println!("  key2_value == EMPTY_WORD: {}", key2_value == EMPTY_WORD);
    println!("  key2_value.is_empty(): {}", key2_value.is_empty());

    assert_eq!(key2_value, EMPTY_WORD, "key2 should be cleared to EMPTY_WORD");
    assert_eq!(map.get(&key3), new_value3, "key3 should be updated");
}
