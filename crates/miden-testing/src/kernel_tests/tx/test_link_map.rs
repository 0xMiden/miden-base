use anyhow::Context;
use miden_objects::{Digest, ONE};
use miden_tx::{host::LinkMap, utils::word_to_masm_push_string};
use vm_processor::{MemAdviceProvider, ProcessState};

use crate::{TransactionContextBuilder, executor::CodeExecutor};

fn is_key_greater(d0: Digest, d1: Digest) -> bool {
    let mut result = 0u8;
    let mut cont = 1;
    let gt = d0[0].as_int() > d1[0].as_int();

    result |= gt as u8;
    cont &= !gt as u8;

    let gt = d0[1].as_int() > d1[1].as_int();
    result |= gt as u8 & cont;
    cont &= !gt as u8 & cont;

    let gt = d0[2].as_int() > d1[2].as_int();
    result |= gt as u8 & cont;
    cont &= !gt as u8 & cont;

    let gt = d0[3].as_int() > d1[3].as_int();
    result |= gt as u8 & cont;

    result == 1
}

#[test]
fn is_greater() -> anyhow::Result<()> {
    for (key0, key1) in [
        ([0, 0, 0, 0u32], [0, 0, 0, 0u32]),
        ([1, 0, 0, 0u32], [0, 0, 0, 0u32]),
        ([0, 1, 0, 0u32], [0, 0, 0, 0u32]),
        ([0, 0, 1, 0u32], [0, 0, 0, 0u32]),
        ([0, 0, 0, 1u32], [0, 0, 0, 0u32]),
        ([0, 0, 0, 0u32], [1, 0, 0, 0u32]),
        ([0, 0, 0, 0u32], [0, 1, 0, 0u32]),
        ([0, 0, 0, 0u32], [0, 0, 1, 0u32]),
        ([0, 0, 0, 0u32], [0, 0, 0, 1u32]),
        ([1, 6, 5, 4u32], [0, 9, 8, 7u32]),
    ]
    .map(|(key0, key1)| (Digest::from(key0), Digest::from(key1)))
    {
        let code = format!(
            r#"
        use.kernel::link_map

        begin
          push.{KEY_1}
          push.{KEY_0}
          # checks if KEY_0 > KEY_1
          exec.link_map::is_greater
          swap drop
        end
        "#,
            KEY_0 = word_to_masm_push_string(&key0),
            KEY_1 = word_to_masm_push_string(&key1),
        );

        let process = CodeExecutor::with_advice_provider(MemAdviceProvider::default())
            .run(&code)
            .unwrap();
        let compare_result = process.stack.get(0);
        let expected = if key0 > key1 { 1 } else { 0 };
        assert_eq!(compare_result.as_int(), expected);
    }

    Ok(())
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
          # push.4.5.6.7
          # key
          # push.1.2.1.1
          # push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          # exec.link_map::set
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
          # push.1.2.1.1
          # push.MAP_PTR
          # => [map_ptr, KEY]

          # exec.link_map::get
          # => [VALUE]

          # push.4.5.6.7
          # assert_eqw.err="retrieved value for key [1, 2, 1, 1] should be the previously inserted value"
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
    let entry1_key = Digest::from([1, 3, 1, 1u32]);
    let entry1_value = Digest::from([2, 3, 4, 5u32]);

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

          # Insert key [1, 3, 1, 1].
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
    );

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let process = tx_context.execute_code(&code).context("failed to execute code")?;
    let state = ProcessState::from(&process);

    let map = LinkMap::new(map_ptr.into(), state).unwrap();
    let mut map_iter = map.iter();

    let first_entry = map_iter.next().expect("map should have two entries");
    let second_entry = map_iter.next().expect("map should have two entries");

    assert_eq!(first_entry.metadata.map_ptr, map_ptr);
    assert_eq!(first_entry.metadata.prev_item, 0);
    assert_eq!(first_entry.metadata.next_item, second_entry.ptr);
    assert_eq!(first_entry.key, *entry0_key);
    assert_eq!(first_entry.value, *entry0_value);

    assert_eq!(second_entry.metadata.map_ptr, map_ptr);
    assert_eq!(second_entry.metadata.prev_item, first_entry.ptr);
    assert_eq!(second_entry.metadata.next_item, 0);
    assert_eq!(second_entry.key, *entry1_key);
    assert_eq!(second_entry.value, *entry1_value);

    Ok(())
}
