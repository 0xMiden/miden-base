use alloc::vec::Vec;
use std::{collections::BTreeMap, string::String};

use anyhow::Context;
use miden_objects::{Digest, ONE, Word};
use miden_tx::{host::LinkMap, utils::word_to_masm_push_string};
use vm_processor::{MemAdviceProvider, ProcessState};
use winter_rand_utils::rand_array;

use crate::{TransactionContextBuilder, executor::CodeExecutor};

enum CompareOperation {
    Less,
    Equal,
    Greater,
}

impl CompareOperation {
    fn procedure_name(&self) -> &'static str {
        match self {
            CompareOperation::Less => "is_less",
            CompareOperation::Equal => "is_equal",
            CompareOperation::Greater => "is_greater",
        }
    }
}

fn execute_comparison_test(operation: CompareOperation) -> anyhow::Result<()> {
    for _ in 0..1000 {
        let key0 = Word::from(rand_array());
        let key1 = Word::from(rand_array());

        let code = format!(
            r#"
        use.kernel::link_map

        begin
          push.{KEY_1}
          push.{KEY_0}
          exec.link_map::{proc_name}
          swap drop
        end
        "#,
            KEY_0 = word_to_masm_push_string(&key0),
            KEY_1 = word_to_masm_push_string(&key1),
            proc_name = operation.procedure_name(),
        );

        let process = CodeExecutor::with_advice_provider(MemAdviceProvider::default())
            .run(&code)
            .unwrap();
        let compare_result = process.stack.get(0);

        let expected = match operation {
            CompareOperation::Less => LinkMap::compare_keys(key0, key1).is_lt(),
            CompareOperation::Equal => LinkMap::compare_keys(key0, key1).is_eq(),
            CompareOperation::Greater => LinkMap::compare_keys(key0, key1).is_gt(),
        };

        let expected_int = if expected { 1 } else { 0 };
        assert_eq!(
            compare_result.as_int(),
            expected_int,
            "failed for procedure {proc_name} with keys {key0:?}, {key1:?}",
            proc_name = operation.procedure_name()
        );
    }

    Ok(())
}

#[test]
fn is_greater() -> anyhow::Result<()> {
    execute_comparison_test(CompareOperation::Greater)
}

#[test]
fn is_equal() -> anyhow::Result<()> {
    execute_comparison_test(CompareOperation::Equal)
}

#[test]
fn is_less() -> anyhow::Result<()> {
    execute_comparison_test(CompareOperation::Less)
}

#[test]
fn set_on_empty_map() -> anyhow::Result<()> {
    let code = r#"
      use.kernel::link_map

      const.MAP_PTR=8
      const.MAP_KEY_OFFSET=4
      const.MAP_VALUE_OFFSET=8

      begin
          # Insert a key-value pair
          # ---------------------------------------------------------------------------------------

          # value
          push.1.2.3.4
          # key
          push.2.3.4.5
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          padw mem_load.MAP_PTR add.MAP_KEY_OFFSET mem_loadw
          push.2.3.4.5 assert_eqw.err="key in memory does not match provided key"
          # => []

          padw mem_load.MAP_PTR add.MAP_VALUE_OFFSET mem_loadw
          push.1.2.3.4 assert_eqw.err="value in memory does not match provided value"
          # => []

          # Get the value at the previously inserted key
          # ---------------------------------------------------------------------------------------

          # key
          push.2.3.4.5
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [VALUE]

          push.1.2.3.4
          assert_eqw.err="retrieved value should be the previously inserted value"
          # => []
      end
    "#;

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    tx_context.execute_code(code).context("failed to execute code")?;

    Ok(())
}

#[test]
fn set_multiple_entries() -> anyhow::Result<()> {
    let code = r#"
      use.kernel::link_map

      const.MAP_PTR=8

      begin
          # Insert key [1, 1, 1, 1].
          # ---------------------------------------------------------------------------------------

          # value
          push.1.2.3.4
          # key
          push.1.1.1.1
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          # Insert key [1, 3, 1, 1].
          # ---------------------------------------------------------------------------------------

          # value
          push.3.4.5.6
          # key
          push.1.3.1.1
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          # Insert key [1, 2, 1, 1].
          # ---------------------------------------------------------------------------------------

          # value
          push.4.5.6.7
          # key
          push.1.2.1.1
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          # Fetch value at key [1, 1, 1, 1].
          # ---------------------------------------------------------------------------------------

          # key
          push.1.1.1.1
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [VALUE]

          push.1.2.3.4
          assert_eqw.err="retrieved value for key [1, 1, 1, 1] should be the previously inserted value"
          # => []

          # Fetch value at key [1, 2, 1, 1].
          # ---------------------------------------------------------------------------------------

          # key
          push.1.2.1.1
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [VALUE]

          push.4.5.6.7
          assert_eqw.err="retrieved value for key [1, 2, 1, 1] should be the previously inserted value"
          # => []

          # Fetch value at key [1, 3, 1, 1].
          # ---------------------------------------------------------------------------------------

          # key
          push.1.3.1.1
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [VALUE]

          push.3.4.5.6
          assert_eqw.err="retrieved value for key [1, 3, 1, 1] should be the previously inserted value"
          # => []
      end
    "#;

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    tx_context.execute_code(code).context("failed to execute code")?;

    Ok(())
}

#[test]
fn link_map_iterator() -> anyhow::Result<()> {
    let map_ptr = 8u32;
    let entry0_key = Digest::from([1, 1, 1, 1u32]);
    let entry0_value = Digest::from([1, 2, 3, 4u32]);
    let entry1_key = Digest::from([1, 2, 1, 1u32]);
    let entry1_value = Digest::from([3, 4, 5, 6u32]);
    let entry2_key = Digest::from([1, 3, 1, 1u32]);
    let entry2_value = Digest::from([2, 3, 4, 5u32]);

    let code = format!(
        r#"
      use.kernel::link_map

      const.MAP_PTR={map_ptr}

      begin
          # Insert key [1, 1, 1, 1].
          # ---------------------------------------------------------------------------------------

          # value
          push.{entry0_value}
          # key
          push.{entry0_key}
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          # Insert key [1, 3, 1, 1] after the previous one.
          # ---------------------------------------------------------------------------------------

          # value
          push.{entry2_value}
          # key
          push.{entry2_key}
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []

          # Insert key [1, 2, 1, 1] in between the first two.
          # ---------------------------------------------------------------------------------------

          # value
          push.{entry1_value}
          # key
          push.{entry1_key}
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => []
      end
    "#,
        entry0_key = word_to_masm_push_string(&entry0_key),
        entry0_value = word_to_masm_push_string(&entry0_value),
        entry1_key = word_to_masm_push_string(&entry1_key),
        entry1_value = word_to_masm_push_string(&entry1_value),
        entry2_key = word_to_masm_push_string(&entry2_key),
        entry2_value = word_to_masm_push_string(&entry2_value),
    );

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();
    let process = tx_context.execute_code(&code).context("failed to execute code")?;
    let state = ProcessState::from(&process);

    let map = LinkMap::new(map_ptr.into(), state).unwrap();
    let mut map_iter = map.iter();

    let first_entry = map_iter.next().expect("map should have three entries");
    let second_entry = map_iter.next().expect("map should have three entries");
    let third_entry = map_iter.next().expect("map should have three entries");
    assert!(map_iter.next().is_none(), "map should only have three entries");

    assert_eq!(first_entry.metadata.map_ptr, map_ptr);
    assert_eq!(first_entry.metadata.prev_item, 0);
    assert_eq!(first_entry.metadata.next_item, second_entry.ptr);
    assert_eq!(first_entry.key, *entry0_key);
    assert_eq!(first_entry.value, *entry0_value);

    assert_eq!(second_entry.metadata.map_ptr, map_ptr);
    assert_eq!(second_entry.metadata.prev_item, first_entry.ptr);
    assert_eq!(second_entry.metadata.next_item, third_entry.ptr);
    assert_eq!(second_entry.key, *entry1_key);
    assert_eq!(second_entry.value, *entry1_value);

    assert_eq!(third_entry.metadata.map_ptr, map_ptr);
    assert_eq!(third_entry.metadata.prev_item, second_entry.ptr);
    assert_eq!(third_entry.metadata.next_item, 0);
    assert_eq!(third_entry.key, *entry2_key);
    assert_eq!(third_entry.value, *entry2_value);

    Ok(())
}

#[test]
fn insert_and_update() -> anyhow::Result<()> {
    let operations = vec![
        TestOperation::insert(digest([1, 0, 0, 0]), digest([1, 2, 3, 4])),
        TestOperation::insert(digest([3, 0, 0, 0]), digest([2, 3, 4, 5])),
        TestOperation::insert(digest([2, 0, 0, 0]), digest([3, 4, 5, 6])),
        TestOperation::insert(digest([1, 0, 0, 0]), digest([4, 5, 6, 7])),
    ];

    execute_link_map_test(operations)
}

#[test]
fn insert_at_head() -> anyhow::Result<()> {
    let operations = vec![
        TestOperation::insert(digest([3, 0, 0, 0]), digest([2, 3, 4, 5])),
        TestOperation::insert(digest([1, 0, 0, 0]), digest([1, 2, 3, 4])),
        TestOperation::insert(digest([2, 0, 0, 0]), digest([3, 4, 5, 6])),
    ];

    execute_link_map_test(operations)
}

fn digest(elements: [u32; 4]) -> Digest {
    Digest::from(elements)
}

enum TestOperation {
    Insert { key: Digest, value: Digest },
}

impl TestOperation {
    pub fn insert(key: Digest, value: Digest) -> Self {
        Self::Insert { key, value }
    }
}

fn execute_link_map_test(operations: Vec<TestOperation>) -> anyhow::Result<()> {
    let mut test_code = String::new();
    let mut control_map = BTreeMap::new();

    for operation in operations {
        match operation {
            TestOperation::Insert { key, value } => {
                control_map.insert(key, value);

                let insertion_code = format!(
                    "
                  push.{value}.{key}.MAP_PTR
                  # => [map_ptr, KEY, NEW_VALUE]
                  exec.link_map::set
                  # => []
                ",
                    key = word_to_masm_push_string(&key),
                    value = word_to_masm_push_string(&value),
                );
                test_code.push_str(&insertion_code);
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
    let map = LinkMap::new(map_ptr.into(), state).unwrap();

    let actual_map: BTreeMap<_, _> = map
        .iter()
        .map(|entry| (Digest::from(entry.key), Digest::from(entry.value)))
        .collect();

    assert_eq!(actual_map.len(), control_map.len());
    for (control_key, control_value) in control_map {
        assert!(actual_map.contains_key(&control_key));
        assert_eq!(actual_map[&control_key], control_value);
    }

    Ok(())
}
