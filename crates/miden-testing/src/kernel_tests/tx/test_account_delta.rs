use alloc::vec::Vec;

use anyhow::Context;
use miden_crypto::{EMPTY_WORD, Word};
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::{AccountBuilder, AccountId, StorageSlot},
    testing::account_component::AccountMockComponent,
    transaction::TransactionScript,
};
use miden_tx::utils::word_to_masm_push_string;

use crate::MockChain;

// ACCOUNT DELTA TESTS
// ================================================================================================

/// Tests that incrementing the nonce by 3 and 2 results in a nonce delta of 5.
#[test]
fn test_delta_nonce() -> anyhow::Result<()> {
    let TestSetup { mock_chain, account_id } = setup_test(vec![]);

    let tx_script = compile_tx_script(format!(
        "
      {TEST_ACCOUNT_CONVENIENCE_WRAPPERS}

      begin
          push.3
          exec.incr_nonce
          # => []

          push.2
          exec.incr_nonce
          # => []
      end
      ",
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .tx_script(tx_script)
        .build()
        .execute()
        .context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce(), Some(miden_objects::Felt::new(5)));

    Ok(())
}

/// Tests that setting a new value for a single value storage slot multiple times results in the
/// correct delta.
///
/// TODO: Test update to the initial value.
#[test]
fn test_storage_delta_for_value_slots() -> anyhow::Result<()> {
    let TestSetup { mock_chain, account_id } =
        setup_test(vec![StorageSlot::Value(EMPTY_WORD), StorageSlot::Value(EMPTY_WORD)]);

    let slot_0_value: Word = miden_objects::Digest::from([6, 7, 8, 9u32]).into();
    let slot_1_value: Word = miden_objects::Digest::from([3, 4, 5, 6u32]).into();

    let tx_script = compile_tx_script(format!(
        "
      {TEST_ACCOUNT_CONVENIENCE_WRAPPERS}

      begin
          push.4.3.2.1
          push.0
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot0_value}
          push.0
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot1_value}
          push.1
          # => [index, VALUE]
          exec.set_item
          # => []

          # nonce must increase for state changing transactions
          push.1 exec.incr_nonce
      end
      ",
        final_slot0_value = word_to_masm_push_string(&slot_0_value),
        final_slot1_value = word_to_masm_push_string(&slot_1_value)
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .tx_script(tx_script)
        .build()
        .execute()
        .context("failed to execute transaction")?;

    let storage_values_delta = executed_tx
        .account_delta()
        .storage()
        .values()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect::<Vec<_>>();
    assert_eq!(storage_values_delta, &[(0u8, slot_0_value), (1u8, slot_1_value),]);

    Ok(())
}

// TEST HELPERS
// ================================================================================================

struct TestSetup {
    mock_chain: MockChain,
    account_id: AccountId,
}

fn setup_test(storage_slots: Vec<StorageSlot>) -> TestSetup {
    let account = AccountBuilder::new([8; 32])
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::testing_assembler(),
                storage_slots,
            )
            .unwrap(),
        )
        .build_existing()
        .unwrap();

    let account_id = account.id();
    let mock_chain = MockChain::with_accounts(&[account]);

    TestSetup { mock_chain, account_id }
}

fn compile_tx_script(code: impl AsRef<str>) -> anyhow::Result<TransactionScript> {
    TransactionScript::compile(
        code.as_ref(),
        [],
        TransactionKernel::testing_assembler_with_mock_account().with_debug_mode(true),
    )
    .context("failed to compile tx script")
}

const TEST_ACCOUNT_CONVENIENCE_WRAPPERS: &str = "
      use.test::account

      #! Inputs:  [nonce_increment]
      #! Outputs: []
      proc.incr_nonce
        repeat.15 push.0 swap end
        # => [nonce_increment, pad(15)]

        call.account::incr_nonce
        # => [pad(16)]

        dropw dropw dropw dropw
      end

      #! Inputs:  [index, VALUE]
      #! Outputs: []
      proc.set_item
          repeat.11 push.0 movdn.5 end

          # => [index, VALUE, pad(11)]

          call.account::set_item
          # => [OLD_VALUE, pad(12)]

          dropw dropw dropw dropw
      end
";
