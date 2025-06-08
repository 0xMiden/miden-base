use alloc::vec::Vec;
use core::cmp::Ordering;
use std::{collections::BTreeMap, string::String};

use anyhow::Context;
use miden_crypto::EMPTY_WORD;
use miden_objects::{Digest, ONE, Word};
use miden_tx::{host::LinkMap, utils::word_to_masm_push_string};
use rand::seq::IteratorRandom;
use vm_processor::{MemAdviceProvider, ProcessState};
use winter_rand_utils::rand_array;

use crate::{TransactionContextBuilder, executor::CodeExecutor};

// TODO: Test multiple link maps at the same time.

/// Tests the following properties:
/// - Insertion into an empty map.
/// - Insertion after an existing entry.
/// - Insertion in between two existing entries.
/// - Insertion before an existing head.
#[test]
fn link_map_iterator() -> anyhow::Result<()> {
    let map_ptr = 8u32;
    // check that using an empty word as key is fine
    let entry0_key = Digest::from([0, 0, 0, 0u32]);
    let entry0_value = Digest::from([1, 2, 3, 4u32]);
    let entry1_key = Digest::from([1, 2, 1, 1u32]);
    let entry1_value = Digest::from([3, 4, 5, 6u32]);
    let entry2_key = Digest::from([1, 3, 1, 1u32]);
    // check that using an empty word as value is fine
    let entry2_value = Digest::from([0, 0, 0, 0u32]);
    let entry3_key = Digest::from([1, 4, 1, 1u32]);
    let entry3_value = Digest::from([5, 6, 7, 8u32]);

    let code = format!(
        r#"
      use.kernel::link_map

      const.MAP_PTR={map_ptr}

      begin
          # Insert key {entry1_key} into an empty map.
          # ---------------------------------------------------------------------------------------

          # value
          padw push.{entry1_value}
          # key
          push.{entry1_key}
          push.MAP_PTR
          # => [map_ptr, KEY, VALUE]

          exec.link_map::set
          # => []

          # Insert key {entry3_key} after the previous one.
          # ---------------------------------------------------------------------------------------

          # value
          padw push.{entry3_value}
          # key
          push.{entry3_key}
          push.MAP_PTR
          # => [map_ptr, KEY, VALUE]

          exec.link_map::set
          # => []

          # Insert key {entry2_key} in between the first two.
          # ---------------------------------------------------------------------------------------

          # value
          padw push.{entry2_value}
          # key
          push.{entry2_key}
          push.MAP_PTR
          # => [map_ptr, KEY, VALUE]

          exec.link_map::set
          # => []

          # Insert key {entry0_key} at the head of the map.
          # ---------------------------------------------------------------------------------------

          # value
          padw push.{entry0_value}
          # key
          push.{entry0_key}
          push.MAP_PTR
          # => [map_ptr, KEY, VALUE]

          exec.link_map::set
          # => []

          # Fetch value at key {entry0_key}.
          # ---------------------------------------------------------------------------------------

          # key
          push.{entry0_key}
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [contains_key, VALUE0, VALUE1]
          assert.err="value for key {entry0_key} should exist"

          push.{entry0_value}
          assert_eqw.err="retrieved value0 for key {entry0_key} should be the previously inserted value"
          padw
          assert_eqw.err="retrieved value1 for key {entry0_key} should be an empty word"
          # => []

          # Fetch value at key {entry1_key}.
          # ---------------------------------------------------------------------------------------

          # key
          push.{entry1_key}
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [contains_key, VALUE0, VALUE1]
          assert.err="value for key {entry1_key} should exist"

          push.{entry1_value}
          assert_eqw.err="retrieved value0 for key {entry1_key} should be the previously inserted value"
          padw
          assert_eqw.err="retrieved value1 for key {entry1_key} should be an empty word"
          # => []

          # Fetch value at key {entry2_key}.
          # ---------------------------------------------------------------------------------------

          # key
          push.{entry2_key}
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [contains_key, VALUE0, VALUE1]
          assert.err="value for key {entry2_key} should exist"

          push.{entry2_value}
          assert_eqw.err="retrieved value0 for key {entry2_key} should be the previously inserted value"
          padw
          assert_eqw.err="retrieved value1 for key {entry2_key} should be an empty word"
          # => []

          # Fetch value at key {entry3_key}.
          # ---------------------------------------------------------------------------------------

          # key
          push.{entry3_key}
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [contains_key, VALUE0, VALUE1]
          assert.err="value for key {entry3_key} should exist"

          push.{entry3_value}
          assert_eqw.err="retrieved value0 for key {entry3_key} should be the previously inserted value"
          padw
          assert_eqw.err="retrieved value1 for key {entry3_key} should be an empty word"
          # => []
      end
    "#,
        entry0_key = word_to_masm_push_string(&entry0_key),
        entry0_value = word_to_masm_push_string(&entry0_value),
        entry1_key = word_to_masm_push_string(&entry1_key),
        entry1_value = word_to_masm_push_string(&entry1_value),
        entry2_key = word_to_masm_push_string(&entry2_key),
        entry2_value = word_to_masm_push_string(&entry2_value),
        entry3_key = word_to_masm_push_string(&entry3_key),
        entry3_value = word_to_masm_push_string(&entry3_value),
    );

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();
    let process = tx_context.execute_code(&code).context("failed to execute code")?;
    let state = ProcessState::from(&process);

    let map = LinkMap::new(map_ptr.into(), state);
    let mut map_iter = map.iter();

    let entry0 = map_iter.next().expect("map should have four entries");
    let entry1 = map_iter.next().expect("map should have four entries");
    let entry2 = map_iter.next().expect("map should have four entries");
    let entry3 = map_iter.next().expect("map should have four entries");
    assert!(map_iter.next().is_none(), "map should only have four entries");

    assert_eq!(entry0.metadata.map_ptr, map_ptr);
    assert_eq!(entry0.metadata.prev_entry_ptr, 0);
    assert_eq!(entry0.metadata.next_entry_ptr, entry1.ptr);
    assert_eq!(entry0.key, *entry0_key);
    assert_eq!(entry0.value0, *entry0_value);
    assert_eq!(entry0.value1, EMPTY_WORD);

    assert_eq!(entry1.metadata.map_ptr, map_ptr);
    assert_eq!(entry1.metadata.prev_entry_ptr, entry0.ptr);
    assert_eq!(entry1.metadata.next_entry_ptr, entry2.ptr);
    assert_eq!(entry1.key, *entry1_key);
    assert_eq!(entry1.value0, *entry1_value);
    assert_eq!(entry1.value1, EMPTY_WORD);

    assert_eq!(entry2.metadata.map_ptr, map_ptr);
    assert_eq!(entry2.metadata.prev_entry_ptr, entry1.ptr);
    assert_eq!(entry2.metadata.next_entry_ptr, entry3.ptr);
    assert_eq!(entry2.key, *entry2_key);
    assert_eq!(entry2.value0, *entry2_value);
    assert_eq!(entry2.value1, EMPTY_WORD);

    assert_eq!(entry3.metadata.map_ptr, map_ptr);
    assert_eq!(entry3.metadata.prev_entry_ptr, entry2.ptr);
    assert_eq!(entry3.metadata.next_entry_ptr, 0);
    assert_eq!(entry3.key, *entry3_key);
    assert_eq!(entry3.value0, *entry3_value);
    assert_eq!(entry3.value1, EMPTY_WORD);

    Ok(())
}

#[test]
fn insert_and_update() -> anyhow::Result<()> {
    let operations = vec![
        TestOperation::set(digest([1, 0, 0, 0]), digest([1, 2, 3, 4])),
        TestOperation::set(digest([3, 0, 0, 0]), digest([2, 3, 4, 5])),
        TestOperation::set(digest([2, 0, 0, 0]), digest([3, 4, 5, 6])),
        // This key is updated.
        TestOperation::set(digest([1, 0, 0, 0]), digest([4, 5, 6, 7])),
    ];

    execute_link_map_test(operations)
}

#[test]
fn insert_at_head() -> anyhow::Result<()> {
    let operations = vec![
        TestOperation::set(digest([3, 0, 0, 0]), digest([2, 3, 4, 5])),
        // These keys are smaller than the existing one, so the head of the map is updated.
        TestOperation::set(digest([1, 0, 0, 0]), digest([1, 2, 3, 4])),
        TestOperation::set(digest([2, 0, 0, 0]), digest([3, 4, 5, 6])),
    ];

    execute_link_map_test(operations)
}

#[test]
fn set_update_get_random_entries() -> anyhow::Result<()> {
    let entries = generate_entries(1000);
    let absent_entries = generate_entries(500);
    let update_ops = generate_updates(&entries, 200);

    // Insert all entries into the map.
    let set_ops = generate_set_ops(&entries);
    // Fetch all values and ensure they are as expected.
    let get_ops = generate_get_ops(&entries);
    // Update a few of the existing keys.
    let set_update_ops = generate_set_ops(&update_ops);
    // Fetch all values and ensure they are as expected, in particular the updated ones.
    let get_ops2 = generate_get_ops(&entries);

    // Fetch values for entries that are (most likely) absent.
    // Note that the link map test will simply assert that the link map returns whatever the btree
    // map returns, so whether they actually exist or not does not matter for the correctness of the
    // test.
    let get_ops3 = generate_get_ops(&absent_entries);

    let mut test_operations = set_ops;
    test_operations.extend(get_ops);
    test_operations.extend(set_update_ops);
    test_operations.extend(get_ops2);
    test_operations.extend(get_ops3);

    execute_link_map_test(test_operations)
}

// COMPARISON OPERATIONS TESTS
// ================================================================================================

#[test]
fn is_key_greater() -> anyhow::Result<()> {
    execute_comparison_test(Ordering::Greater)
}

#[test]
fn is_key_equal() -> anyhow::Result<()> {
    execute_comparison_test(Ordering::Equal)
}

#[test]
fn is_key_less() -> anyhow::Result<()> {
    execute_comparison_test(Ordering::Less)
}

fn execute_comparison_test(operation: Ordering) -> anyhow::Result<()> {
    let procedure_name = match operation {
        Ordering::Less => "is_key_less",
        Ordering::Equal => "is_key_equal",
        Ordering::Greater => "is_key_greater",
    };

    let mut test_code = String::new();

    for _ in 0..1000 {
        let key0 = Word::from(rand_array());
        let key1 = Word::from(rand_array());

        let cmp = LinkMap::compare_keys(key0, key1);
        let expected = cmp == operation;

        let code = format!(
            r#"
        push.{KEY_1}
        push.{KEY_0}
        exec.link_map::{proc_name}
        push.{expected_value}
        assert_eq.err="failed for procedure {proc_name} with keys {key0:?}, {key1:?}"
      "#,
            KEY_0 = word_to_masm_push_string(&key0),
            KEY_1 = word_to_masm_push_string(&key1),
            proc_name = procedure_name,
            expected_value = expected as u8
        );

        test_code.push_str(&code);
    }

    let code = format!(
        r#"
        use.kernel::link_map

        begin
          {test_code}
        end
        "#,
    );

    CodeExecutor::with_advice_provider(MemAdviceProvider::default())
        .run(&code)
        .with_context(|| format!("comparion test for {procedure_name} failed"))?;

    Ok(())
}

// TEST HELPERS
// ================================================================================================

fn digest(elements: [u32; 4]) -> Digest {
    Digest::from(elements)
}

enum TestOperation {
    Set { key: Digest, value: Digest },
    Get { key: Digest },
}

impl TestOperation {
    pub fn set(key: Digest, value: Digest) -> Self {
        Self::Set { key, value }
    }
    pub fn get(key: Digest) -> Self {
        Self::Get { key }
    }
}

// TODO: Implement passing a double word as value instead of one word.
fn execute_link_map_test(operations: Vec<TestOperation>) -> anyhow::Result<()> {
    let mut test_code = String::new();
    let mut control_map = BTreeMap::new();

    for operation in operations {
        match operation {
            TestOperation::Set { key, value } => {
                control_map.insert(key, value);

                let set_code = format!(
                    "
                  padw push.{value}.{key}.MAP_PTR
                  # => [map_ptr, KEY, VALUE]
                  exec.link_map::set
                  # => []
                ",
                    key = word_to_masm_push_string(&key),
                    value = word_to_masm_push_string(&value),
                );
                test_code.push_str(&set_code);
            },
            TestOperation::Get { key } => {
                let control_value = control_map.get(&key);

                let (expected_contains_key, expected_value) = match control_value {
                    Some(value) => (true, *value),
                    None => (false, Digest::from(EMPTY_WORD)),
                };

                let get_code = format!(
                    r#"
                  push.{key}.MAP_PTR
                  # => [map_ptr, KEY]
                  exec.link_map::get
                  # => [contains_key, VALUE0, VALUE1]
                  push.{expected_contains_key}
                  assert_eq.err="contains_key did not match the expected value: {expected_contains_key}"
                  push.{expected_value}
                  assert_eqw.err="value returned from get is not the expected value: {expected_value}"
                  dropw
                "#,
                    key = word_to_masm_push_string(&key),
                    expected_value = word_to_masm_push_string(&expected_value),
                    expected_contains_key = expected_contains_key as u8
                );

                test_code.push_str(&get_code);
            },
        }
    }

    let map_ptr = 8u32;

    let code = format!(
        r#"
      use.kernel::link_map

      const.MAP_PTR={map_ptr}

      begin
          {test_code}
      end
    "#
    );

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();
    let process = tx_context.execute_code(&code).context("failed to execute code")?;
    let state = ProcessState::from(&process);
    let map = LinkMap::new(map_ptr.into(), state);

    let actual_map_len = map.iter().count();

    assert_eq!(actual_map_len, control_map.len());

    // The order of the entries in the control map should be the same as what the link map returns.
    let mut control_entries: Vec<_> = control_map.into_iter().collect();
    control_entries.sort_by(|(key0, _), (key1, _)| {
        LinkMap::compare_keys(Word::from(*key0), Word::from(*key1))
    });

    for ((control_key, control_value), (actual_key, actual_value)) in control_entries
        .into_iter()
        .zip(map.iter().map(|entry| (Digest::from(entry.key), Digest::from(entry.value0))))
    {
        assert_eq!(actual_key, control_key);
        assert_eq!(actual_value, control_value);
    }

    Ok(())
}

fn generate_set_ops(entries: &[(Digest, Digest)]) -> Vec<TestOperation> {
    entries.iter().map(|(key, value)| TestOperation::set(*key, *value)).collect()
}

fn generate_get_ops(entries: &[(Digest, Digest)]) -> Vec<TestOperation> {
    entries.iter().map(|(key, _)| TestOperation::get(*key)).collect()
}

fn generate_entries(count: u64) -> Vec<(Digest, Digest)> {
    (0..count)
        .map(|_| {
            let key = rand_digest();
            let value = rand_digest();
            (key, value)
        })
        .collect()
}

fn generate_updates(entries: &[(Digest, Digest)], num_updates: usize) -> Vec<(Digest, Digest)> {
    let mut rng = rand::rng();

    entries
        .iter()
        .choose_multiple(&mut rng, num_updates)
        .into_iter()
        .map(|(key, _)| (*key, rand_digest()))
        .collect()
}

fn rand_digest() -> Digest {
    Digest::new(rand_array())
}
