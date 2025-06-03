use anyhow::Context;
use miden_objects::ONE;

use crate::TransactionContextBuilder;

#[test]
fn link_map_set_get() -> anyhow::Result<()> {
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
fn new_set_get() -> anyhow::Result<()> {
    let code = r#"
      use.kernel::link_map

      const.MAP_PTR=8

      begin
          # Initialize a new map
          # ---------------------------------------------------------------------------------------

          push.MAP_PTR exec.link_map::new
          # => []

          # Insert a key-value pair
          # ---------------------------------------------------------------------------------------

          # value
          push.1.2.3.4
          # key
          push.5.6.7.8
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => [OLD_VALUE]

          padw assert_eqw.err="old value should be the empty word"
          # => []

          # Overwrite the previously inserted key-value pair
          # ---------------------------------------------------------------------------------------

          # value
          push.9.8.7.6
          # key
          push.5.6.7.8
          push.MAP_PTR
          # => [map_ptr, KEY, NEW_VALUE]

          exec.link_map::set
          # => [OLD_VALUE]

          push.1.2.3.4
          assert_eqw.err="old value should be the previously inserted value"
          # => []

          # Get the value at the previously inserted key
          # ---------------------------------------------------------------------------------------

          # key
          push.5.6.7.8
          push.MAP_PTR
          # => [map_ptr, KEY]

          exec.link_map::get
          # => [VALUE]

          push.9.8.7.6
          assert_eqw.err="retrieved value should be the previously inserted value"
          # => []

          # Compute the map commitment
          # ---------------------------------------------------------------------------------------

          # for now we only assert that a value is returned
          push.MAP_PTR exec.link_map::compute_commitment
          # => [MAP_COMMITMENT]

          dropw
          # => []
      end
    "#;

    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    tx_context.execute_code(code).context("failed to execute code")?;

    Ok(())
}
