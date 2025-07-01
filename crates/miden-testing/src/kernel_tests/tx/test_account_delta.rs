use alloc::vec::Vec;

use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    Digest, EMPTY_WORD, Felt, Hasher, Word,
    account::{
        AccountBuilder, AccountDelta, AccountHeader, AccountId, AccountStorageMode, StorageSlot,
    },
    testing::account_component::AccountMockComponent,
    transaction::{ExecutedTransaction, TransactionScript},
    vm::AdviceMap,
};
use miden_tx::{TransactionExecutorError, utils::word_to_masm_push_string};

use crate::MockChain;

// ACCOUNT DELTA TESTS
// ================================================================================================
// TODO:
// - Add test for calling account_delta::compute_commitment from foreign account and make sure it
//   returns the correct value (i.e. no part of the computation is using foreign account data).

/// Tests that incrementing the nonce by 3 and 2 results in a nonce delta of 5.
#[test]
fn delta_nonce() -> anyhow::Result<()> {
    let TestSetup { mock_chain, account_id } = setup_test(vec![]);

    let tx_script = compile_tx_script(
        "
      begin
          push.3
          exec.incr_nonce
          # => []

          push.2
          exec.incr_nonce
          # => []
      end
      ",
    )?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .expect("failed to build tx context")
        .tx_script(tx_script)
        .build()
        .execute()
        .context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce(), Some(Felt::new(5)));

    validate_account_delta(&executed_tx).context("failed to validate delta")?;

    Ok(())
}

/// Tests that setting new values for value storage slots results in the correct delta.
#[test]
fn storage_delta_for_value_slots() -> anyhow::Result<()> {
    // Slot 0 is updated from non-empty word to empty word.
    let slot_0_init_value = word([2, 4, 6, 8u32]);
    let slot_0_final_value = EMPTY_WORD;

    // Slot 1 is updated from empty word to non-empty word.
    let slot_1_init_value = EMPTY_WORD;
    let slot_1_final_value = word([3, 4, 5, 6u32]);

    // Slot 2 is updated to itself.
    let slot_2_init_value = word([1, 3, 5, 7u32]);
    let slot_2_final_value = slot_2_init_value;

    let TestSetup { mock_chain, account_id } = setup_test(vec![
        StorageSlot::Value(slot_0_init_value),
        StorageSlot::Value(slot_1_init_value),
        StorageSlot::Value(slot_2_init_value),
    ]);

    let tx_script = compile_tx_script(format!(
        "
      begin
          push.{tmp_slot_0_value}
          push.0
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot_0_value}
          push.0
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot_1_value}
          push.1
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot_2_value}
          push.2
          # => [index, VALUE]
          exec.set_item
          # => []

          # nonce must increase for state changing transactions
          push.1 exec.incr_nonce
      end
      ",
        // Set slot 0 to some other value initially.
        tmp_slot_0_value = word_to_masm_push_string(&slot_1_final_value),
        final_slot_0_value = word_to_masm_push_string(&slot_0_final_value),
        final_slot_1_value = word_to_masm_push_string(&slot_1_final_value),
        final_slot_2_value = word_to_masm_push_string(&slot_2_final_value)
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .expect("failed to build tx context")
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

    // Note that slot 2 is absent because its value hasn't changed.
    assert_eq!(storage_values_delta, &[(0u8, slot_0_final_value), (1u8, slot_1_final_value)]);

    validate_account_delta(&executed_tx).context("failed to validate delta")?;

    Ok(())
}

/// Validates that the given host-computed account delta has the same commitment as the in-kernel
/// computed account delta.
///
/// TODO: This will eventually be done in `build_executed_transaction`.
fn validate_account_delta(
    executed_tx: &ExecutedTransaction,
) -> Result<(), TransactionExecutorError> {
    let account_delta: &AccountDelta = executed_tx.account_delta();
    let advice_map: &AdviceMap = &executed_tx.advice_witness().map;
    let final_account_header: &AccountHeader = executed_tx.final_account();

    let host_delta_commitment = account_delta.commitment();
    let account_update_commitment =
        Hasher::merge(&[final_account_header.commitment(), host_delta_commitment]);

    let account_update_data = advice_map.get(&account_update_commitment).ok_or_else(|| {
        TransactionExecutorError::AccountUpdateCommitment(
            "failed to find ACCOUNT_UPDATE_COMMITMENT in advice map",
        )
    })?;

    if account_update_data.len() != 8 {
        return Err(TransactionExecutorError::AccountUpdateCommitment(
            "expected account update commitment advice map entry to contain exactly 8 elements",
        ));
    }

    // SAFETY: We just asserted that the data is of length 8 so slicing the data into two words
    // is fine.
    // TODO: The final account commitment will eventually be taken from here once the account update
    // commitment becomes a transaction output, but for now it is unused.
    let _final_account_commitment = Digest::from(
        <[Felt; 4]>::try_from(&account_update_data[0..4])
            .expect("we should have sliced off exactly four elements"),
    );
    let account_delta_commitment = Digest::from(
        <[Felt; 4]>::try_from(&account_update_data[4..8])
            .expect("we should have sliced off exactly four elements"),
    );

    if account_delta_commitment != host_delta_commitment {
        return Err(TransactionExecutorError::InconsistentAccountDeltaCommitment {
            // TODO: Update once in kernel commitment is read from tx outputs.
            in_kernel_commitment: Digest::from(EMPTY_WORD),
            host_commitment: host_delta_commitment,
        });
    }

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
        .storage_mode(AccountStorageMode::Public)
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
    let mock_chain = MockChain::with_accounts(&[account]).expect("valid setup");

    TestSetup { mock_chain, account_id }
}

fn compile_tx_script(code: impl AsRef<str>) -> anyhow::Result<TransactionScript> {
    let code = format!(
        "
    {TEST_ACCOUNT_CONVENIENCE_WRAPPERS}
    {code}
    ",
        code = code.as_ref()
    );

    TransactionScript::compile(
        &code,
        TransactionKernel::testing_assembler_with_mock_account().with_debug_mode(true),
    )
    .context("failed to compile tx script")
}

fn word(data: [u32; 4]) -> Word {
    Word::from(Digest::from(data))
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
