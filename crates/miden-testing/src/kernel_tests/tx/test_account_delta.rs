use alloc::vec::Vec;
use std::collections::BTreeMap;

use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    Digest, EMPTY_WORD, Felt, Hasher, Word,
    account::{
        AccountBuilder, AccountDelta, AccountHeader, AccountId, AccountStorageMode, AccountType,
        StorageSlot,
    },
    asset::{Asset, FungibleAsset},
    note::{Note, NoteType},
    testing::{account_component::AccountMockComponent, account_id::AccountIdBuilder},
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
    let TestSetup { mock_chain, account_id } = setup_storage_test(vec![]);

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

    let TestSetup { mock_chain, account_id } = setup_storage_test(vec![
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

/// Tests that increasing, decreasing the amount of a fungible asset results in the correct delta.
/// - Asset0 is increased by 100 and decreased by 200 -> Delta: -100.
/// - Asset1 is increased by 100 and decreased by 100 -> Delta: 0.
/// - Asset2 is increased by 200 and decreased by 100 -> Delta: 100.
#[test]
fn fungible_asset_delta() -> anyhow::Result<()> {
    // Test with random IDs to make sure the ordering in the MASM and Rust implementations
    // matches.
    let faucet0: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .build_with_seed(rand::random());
    let faucet1: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .build_with_seed(rand::random());
    let faucet2: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .build_with_seed(rand::random());

    let original_asset0 = FungibleAsset::new(faucet0, 300)?;
    let original_asset1 = FungibleAsset::new(faucet1, 200)?;
    let original_asset2 = FungibleAsset::new(faucet2, 100)?;

    let added_asset0 = FungibleAsset::new(faucet0, 100)?;
    let added_asset1 = FungibleAsset::new(faucet1, 100)?;
    let added_asset2 = FungibleAsset::new(faucet2, 200)?;

    let removed_asset0 = FungibleAsset::new(faucet0, 200)?;
    let removed_asset1 = FungibleAsset::new(faucet1, 100)?;
    let removed_asset2 = FungibleAsset::new(faucet2, 100)?;

    let TestSetup { mut mock_chain, account_id } =
        setup_asset_test([original_asset0, original_asset1, original_asset2].map(Asset::from));

    let mut added_notes = vec![];
    for added_asset in [added_asset0, added_asset1, added_asset2] {
        let added_note = mock_chain
            .add_pending_p2id_note(
                account_id,
                account_id,
                &[Asset::from(added_asset)],
                NoteType::Public,
            )
            .context("failed to add note with asset")?;
        added_notes.push(added_note);
    }
    mock_chain.prove_next_block();

    let tx_script = compile_tx_script(format!(
        "
    begin
        push.{asset0} exec.create_note_with_asset
        # => []
        push.{asset1} exec.create_note_with_asset
        # => []
        push.{asset2} exec.create_note_with_asset
        # => []

        # nonce must increase for state changing transactions
        push.1 exec.incr_nonce
    end
    ",
        asset0 = word_to_masm_push_string(&removed_asset0.into()),
        asset1 = word_to_masm_push_string(&removed_asset1.into()),
        asset2 = word_to_masm_push_string(&removed_asset2.into()),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &added_notes.iter().map(Note::id).collect::<Vec<_>>(), &[])
        .tx_script(tx_script)
        .build()
        .execute()
        .context("failed to execute transaction")?;

    let mut added_assets = executed_tx
        .account_delta()
        .vault()
        .added_assets()
        .map(|asset| (Digest::from(asset.vault_key()), asset.unwrap_fungible().amount()))
        .collect::<BTreeMap<_, _>>();
    let mut removed_assets = executed_tx
        .account_delta()
        .vault()
        .removed_assets()
        .map(|asset| (Digest::from(asset.vault_key()), asset.unwrap_fungible().amount()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(added_assets.len(), 1);
    assert_eq!(removed_assets.len(), 1);

    assert_eq!(
        added_assets.remove(&Digest::from(added_asset2.vault_key())).unwrap(),
        added_asset2.amount() - removed_asset2.amount()
    );
    assert_eq!(
        removed_assets.remove(&Digest::from(added_asset0.vault_key())).unwrap(),
        removed_asset0.amount() - added_asset0.amount()
    );

    validate_account_delta(&executed_tx)?;

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

fn setup_storage_test(storage_slots: Vec<StorageSlot>) -> TestSetup {
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
    let mock_chain = MockChain::with_accounts(&[account]);

    TestSetup { mock_chain, account_id }
}

fn setup_asset_test(assets: impl IntoIterator<Item = Asset>) -> TestSetup {
    let account = AccountBuilder::new([3; 32])
        .storage_mode(AccountStorageMode::Public)
        .with_component(
            AccountMockComponent::new_with_slots(TransactionKernel::testing_assembler(), vec![])
                .unwrap(),
        )
        .with_assets(assets)
        .build_existing()
        .unwrap();

    let account_id = account.id();
    let mock_chain = MockChain::with_accounts(&[account]);

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

      #! Inputs:  [ASSET]
      #! Outputs: []
      proc.create_note_with_asset
          push.0.1.2.3           # recipient
          push.1                 # note_execution_hint
          push.2                 # note_type private
          push.0                 # aux
          push.0xC0000000        # tag
          # => [tag, aux, note_type, execution_hint, RECIPIENT, ASSET]

          exec.create_note
          # => [note_idx, ASSET]

          movdn.4
          # => [ASSET, note_idx]

          exec.move_asset_to_note
          # => []
      end

      #! Inputs:  [tag, aux, note_type, execution_hint, RECIPIENT]
      #! Outputs: [note_idx]
      proc.create_note
          repeat.8 push.0 movdn.8 end
          # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8)]

          call.account::create_note
          # => [note_idx, pad(15)]

          repeat.15 swap drop end
          # => [note_idx]
      end

      #! Inputs:  [ASSET, note_idx]
      #! Outputs: []
      proc.move_asset_to_note
          repeat.11 push.0 movdn.5 end
          # => [ASSET, note_idx, pad(11)]

          call.account::move_asset_to_note

          # return values are unused
          dropw dropw dropw dropw
      end
";
