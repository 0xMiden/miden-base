use alloc::vec::Vec;
use core::cmp::Ordering;
use std::{collections::BTreeMap, string::String};

use anyhow::Context;
use miden_objects::{Digest, EMPTY_WORD, Word};
use miden_tx::{host::LinkMap, utils::word_to_masm_push_string};
use rand::seq::IteratorRandom;
use vm_processor::{MemAdviceProvider, ProcessState};
use winter_rand_utils::rand_array;

use crate::{TransactionContextBuilder, executor::CodeExecutor};

/// Tests the following properties:
/// - Insertion into an empty map.
/// - Insertion after an existing entry.
/// - Insertion in between two existing entries.
/// - Insertion before an existing head.
#[test]
fn insertion() -> anyhow::Result<()> {
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
          # => [is_new_key]
          assert.err="{entry1_key} should be a new key in the map"
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
          # => [is_new_key]
          assert.err="{entry3_key} should be a new key in the map"
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
          # => [is_new_key]
          assert.err="{entry2_key} should be a new key in the map"
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
          # => [is_new_key]
          assert.err="{entry0_key} should be a new key in the map"
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

    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
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
    const MAP_PTR: u32 = 8;

    let value0 = digest([1, 2, 3, 4]);
    let value1 = digest([2, 3, 4, 5]);
    let value2 = digest([3, 4, 5, 6]);

    let operations = vec![
        TestOperation::set(MAP_PTR, digest([1, 0, 0, 0]), (value0, value1)),
        TestOperation::set(MAP_PTR, digest([3, 0, 0, 0]), (value1, value2)),
        TestOperation::set(MAP_PTR, digest([2, 0, 0, 0]), (value2, value1)),
        // This key is updated.
        TestOperation::set(MAP_PTR, digest([1, 0, 0, 0]), (value1, value1)),
        // This key is updated (even though its value is the same).
        TestOperation::set(MAP_PTR, digest([3, 0, 0, 0]), (value1, value2)),
    ];

    execute_link_map_test(operations)
}

#[test]
fn insert_at_head() -> anyhow::Result<()> {
    const MAP_PTR: u32 = 8;

    let key3 = digest([3, 0, 0, 0]);
    let key2 = digest([2, 0, 0, 0]);
    let key1 = digest([1, 0, 0, 0]);
    let value0 = digest([1, 2, 3, 4]);
    let value1 = digest([2, 3, 4, 5]);
    let value2 = digest([3, 4, 5, 6]);

    let operations = vec![
        TestOperation::set(MAP_PTR, key3, (value1, value0)),
        // These keys are smaller than the existing one, so the head of the map is updated.
        TestOperation::set(MAP_PTR, key2, (value2, value0)),
        TestOperation::set(MAP_PTR, key1, (value1, value2)),
        TestOperation::get(MAP_PTR, key1),
        TestOperation::get(MAP_PTR, key2),
        TestOperation::get(MAP_PTR, key3),
    ];

    execute_link_map_test(operations)
}

/// Tests that a get before a set results in the expected returned values and behavior.
#[test]
fn get_before_set() -> anyhow::Result<()> {
    const MAP_PTR: u32 = 8;

    let key0 = digest([3, 0, 0, 0]);
    let value0 = digest([1, 2, 3, 4]);
    let value1 = digest([2, 3, 4, 5]);

    let operations = vec![
        TestOperation::get(MAP_PTR, key0),
        TestOperation::set(MAP_PTR, key0, (value1, value0)),
        TestOperation::get(MAP_PTR, key0),
    ];

    execute_link_map_test(operations)
}

#[test]
fn multiple_link_maps() -> anyhow::Result<()> {
    const MAP_PTR0: u32 = 8;
    const MAP_PTR1: u32 = 12;

    let key3 = digest([3, 0, 0, 0]);
    let key2 = digest([2, 0, 0, 0]);
    let key1 = digest([1, 0, 0, 0]);
    let value0 = digest([1, 2, 3, 4]);
    let value1 = digest([2, 3, 4, 5]);
    let value2 = digest([3, 4, 5, 6]);

    let operations = vec![
        TestOperation::set(MAP_PTR0, key3, (value0, value2)),
        TestOperation::set(MAP_PTR0, key2, (value1, value2)),
        TestOperation::set(MAP_PTR1, key1, (value2, value2)),
        TestOperation::set(MAP_PTR1, key3, (value0, value2)),
        // Note that not all keys that we fetch have been inserted, but that is intentional.
        TestOperation::get(MAP_PTR0, key1),
        TestOperation::get(MAP_PTR0, key2),
        TestOperation::get(MAP_PTR0, key3),
        TestOperation::get(MAP_PTR1, key1),
        TestOperation::get(MAP_PTR1, key2),
        TestOperation::get(MAP_PTR1, key3),
    ];

    execute_link_map_test(operations)
}

#[test]
fn set_update_get_random_entries() -> anyhow::Result<()> {
    const MAP_PTR: u32 = 12;

    let entries = generate_entries(1000);
    let absent_entries = generate_entries(500);
    let update_ops = generate_updates(&entries, 200);

    // Insert all entries into the map.
    let set_ops = generate_set_ops(MAP_PTR, &entries);
    // Fetch all values and ensure they are as expected.
    let get_ops = generate_get_ops(MAP_PTR, &entries);
    // Update a few of the existing keys.
    let set_update_ops = generate_set_ops(MAP_PTR, &update_ops);
    // Fetch all values and ensure they are as expected, in particular the updated ones.
    let get_ops2 = generate_get_ops(MAP_PTR, &entries);

    // Fetch values for entries that are (most likely) absent.
    // Note that the link map test will simply assert that the link map returns whatever the
    // BTreeMap returns, so whether they actually exist or not does not matter for the correctness
    // of the test.
    let get_ops3 = generate_get_ops(MAP_PTR, &absent_entries);

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
fn is_key_less() -> anyhow::Result<()> {
    execute_comparison_test(Ordering::Less)
}

fn execute_comparison_test(operation: Ordering) -> anyhow::Result<()> {
    let procedure_name = match operation {
        Ordering::Less => "is_key_less",
        Ordering::Equal => anyhow::bail!("unsupported ordering operation for testing"),
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
    Set {
        map_ptr: u32,
        key: Digest,
        value0: Digest,
        value1: Digest,
    },
    Get {
        map_ptr: u32,
        key: Digest,
    },
}

impl TestOperation {
    pub fn set(map_ptr: u32, key: Digest, values: (Digest, Digest)) -> Self {
        Self::Set {
            map_ptr,
            key,
            value0: values.0,
            value1: values.1,
        }
    }
    pub fn get(map_ptr: u32, key: Digest) -> Self {
        Self::Get { map_ptr, key }
    }
}

// TODO: Implement passing a double word as value instead of one word.
fn execute_link_map_test(operations: Vec<TestOperation>) -> anyhow::Result<()> {
    let mut test_code = String::new();
    let mut control_maps = BTreeMap::new();

    for operation in operations {
        match operation {
            TestOperation::Set { map_ptr, key, value0, value1 } => {
                let control_map: &mut BTreeMap<_, _> = control_maps.entry(map_ptr).or_default();
                let is_new_key = control_map.insert(key, (value0, value1)).is_none();

                let set_code = format!(
                    r#"
                  push.{value1}.{value0}.{key}.{map_ptr}
                  # => [map_ptr, KEY, VALUE]
                  exec.link_map::set
                  # => [is_new_key]
                  push.{expected_is_new_key}
                  assert_eq.err="is_new_key returned by link_map::set for {key} did not match expected value {expected_is_new_key}"
                "#,
                    key = word_to_masm_push_string(&key),
                    value0 = word_to_masm_push_string(&value0),
                    value1 = word_to_masm_push_string(&value1),
                    expected_is_new_key = is_new_key as u8,
                );

                test_code.push_str(&set_code);
            },
            TestOperation::Get { map_ptr, key } => {
                let control_map: &mut BTreeMap<_, _> = control_maps.entry(map_ptr).or_default();
                let control_value = control_map.get(&key);

                let (expected_contains_key, (expected_value0, expected_value1)) =
                    match control_value {
                        Some(value) => (true, (value.0, value.1)),
                        None => (false, (Digest::from(EMPTY_WORD), Digest::from(EMPTY_WORD))),
                    };

                let get_code = format!(
                    r#"
                  push.{key}.{map_ptr}
                  # => [map_ptr, KEY]
                  exec.link_map::get
                  # => [contains_key, VALUE0, VALUE1]
                  push.{expected_contains_key}
                  assert_eq.err="contains_key did not match the expected value: {expected_contains_key}"
                  push.{expected_value0}
                  assert_eqw.err="value0 returned from get is not the expected value: {expected_value0}"
                  push.{expected_value1}
                  assert_eqw.err="value1 returned from get is not the expected value: {expected_value1}"
                "#,
                    key = word_to_masm_push_string(&key),
                    expected_value0 = word_to_masm_push_string(&expected_value0),
                    expected_value1 = word_to_masm_push_string(&expected_value1),
                    expected_contains_key = expected_contains_key as u8
                );

                test_code.push_str(&get_code);
            },
        }
    }

    let code = format!(
        r#"
      use.kernel::link_map
      begin
          {test_code}
      end
    "#
    );

    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let process = tx_context.execute_code(&code).context("failed to execute code")?;
    let state = ProcessState::from(&process);

    for (map_ptr, control_map) in control_maps {
        let map = LinkMap::new(map_ptr.into(), state);
        let actual_map_len = map.iter().count();
        assert_eq!(
            actual_map_len,
            control_map.len(),
            "size of link map {map_ptr} is different from control map"
        );

        // The order of the entries in the control map should be the same as what the link map
        // returns.
        let mut control_entries: Vec<_> = control_map.into_iter().collect();
        control_entries.sort_by(|(key0, _), (key1, _)| {
            LinkMap::compare_keys(Word::from(*key0), Word::from(*key1))
        });

        for (
            idx,
            (
                (control_key, (control_value0, control_value1)),
                (actual_key, (actual_value0, actual_value1)),
            ),
        ) in control_entries
            .into_iter()
            .zip(map.iter().map(|entry| {
                (
                    Digest::from(entry.key),
                    (Digest::from(entry.value0), Digest::from(entry.value1)),
                )
            }))
            .enumerate()
        {
            assert_eq!(
                actual_key, control_key,
                "link map {map_ptr}'s key is different from control map's key at index {idx}"
            );
            assert_eq!(
                actual_value0, control_value0,
                "link map {map_ptr}'s value0 is different from control map's value0 at index {idx}"
            );
            assert_eq!(
                actual_value1, control_value1,
                "link map {map_ptr}'s value1 is different from control map's value1 at index {idx}"
            );
        }
    }

    Ok(())
}

fn generate_set_ops(map_ptr: u32, entries: &[(Digest, (Digest, Digest))]) -> Vec<TestOperation> {
    entries
        .iter()
        .map(|(key, values)| TestOperation::set(map_ptr, *key, *values))
        .collect()
}

fn generate_get_ops(map_ptr: u32, entries: &[(Digest, (Digest, Digest))]) -> Vec<TestOperation> {
    entries.iter().map(|(key, _)| TestOperation::get(map_ptr, *key)).collect()
}

fn generate_entries(count: u64) -> Vec<(Digest, (Digest, Digest))> {
    (0..count)
        .map(|_| {
            let key = rand_digest();
            let value0 = rand_digest();
            let value1 = rand_digest();
            (key, (value0, value1))
        })
        .collect()
}

fn generate_updates(
    entries: &[(Digest, (Digest, Digest))],
    num_updates: usize,
) -> Vec<(Digest, (Digest, Digest))> {
    let mut rng = rand::rng();

    entries
        .iter()
        .choose_multiple(&mut rng, num_updates)
        .into_iter()
        .map(|(key, _)| (*key, (rand_digest(), rand_digest())))
        .collect()
}

fn rand_digest() -> Digest {
    Digest::new(rand_array())
}
