use alloc::vec::Vec;
use std::collections::BTreeMap;

use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    Digest, EMPTY_WORD, Felt, Hasher, Word,
    account::{
        AccountBuilder, AccountDelta, AccountHeader, AccountId, AccountStorageMode, AccountType,
        StorageMap, StorageSlot, delta::LexicographicWord,
    },
    asset::{Asset, FungibleAsset},
    note::{Note, NoteType},
    testing::{
        account_component::AccountMockComponent, account_id::AccountIdBuilder,
        asset::NonFungibleAssetBuilder,
    },
    transaction::{ExecutedTransaction, TransactionScript},
    vm::AdviceMap,
};
use miden_tx::{TransactionExecutorError, utils::word_to_masm_push_string};
use rand::Rng;

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

    assert_eq!(executed_tx.account_delta().nonce_increment(), Felt::new(5));

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

/// Tests that setting new values for value storage slots results in the correct delta.
/// - Slot 0: key0: EMPTY_WORD -> [1,2,3,4]              -> Delta: [1,2,3,4]
/// - Slot 0: key1: EMPTY_WORD -> [1,2,3,4] -> [2,3,4,5] -> Delta: [2,3,4,5]
/// - Slot 1: key2: [1,2,3,4]  -> [1,2,3,4]              -> Delta: None
/// - Slot 1: key3: [1,2,3,4]  -> EMPTY_WORD             -> Delta: EMPTY_WORD
/// - TODO (once account delta tracker is updated):
///   - Slot 1: key4: [1,2,3,4]  -> [2,3,4,5] -> [1,2,3,4] -> Delta: None
#[test]
fn storage_delta_for_map_slots() -> anyhow::Result<()> {
    // Test with random keys to make sure the ordering in the MASM and Rust implementations
    // matches.
    let key0 = Digest::from(word(winter_rand_utils::rand_array()));
    let key1 = Digest::from(word(winter_rand_utils::rand_array()));
    let key2 = Digest::from(word(winter_rand_utils::rand_array()));
    let key3 = Digest::from(word(winter_rand_utils::rand_array()));

    let key0_init_value = EMPTY_WORD;
    let key1_init_value = EMPTY_WORD;
    let key2_init_value = word([1, 2, 3, 4u32]);
    let key3_init_value = word([1, 2, 3, 4u32]);

    let key0_final_value = word([1, 2, 3, 4u32]);
    let key1_tmp_value = word([1, 2, 3, 4u32]);
    let key1_final_value = word([2, 3, 4, 5u32]);
    let key2_final_value = key2_init_value;
    let key3_final_value = EMPTY_WORD;

    let mut map0 = StorageMap::new();
    map0.insert(key0, key0_init_value);
    map0.insert(key1, key1_init_value);

    let mut map1 = StorageMap::new();
    map1.insert(key2, key2_init_value);
    map1.insert(key3, key3_init_value);

    let TestSetup { mock_chain, account_id } = setup_storage_test(vec![
        StorageSlot::Map(map0),
        StorageSlot::Map(map1),
        // Include an empty map which does not receive any updates, to test that the "metadata
        // header" in the delta commitemnt is not appended if there are no updates to a map
        // slot.
        StorageSlot::Map(StorageMap::new()),
    ]);

    let tx_script = compile_tx_script(format!(
        "
      begin
          push.{key0_value}.{key0}.0
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key1_tmp_value}.{key1}.0
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key1_value}.{key1}.0
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key2_value}.{key2}.1
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key3_value}.{key3}.1
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          # nonce must increase for state changing transactions
          push.1 exec.incr_nonce
      end
      ",
        key0 = word_to_masm_push_string(&key0),
        key1 = word_to_masm_push_string(&key1),
        key2 = word_to_masm_push_string(&key2),
        key3 = word_to_masm_push_string(&key3),
        key0_value = word_to_masm_push_string(&key0_final_value),
        key1_tmp_value = word_to_masm_push_string(&key1_tmp_value),
        key1_value = word_to_masm_push_string(&key1_final_value),
        key2_value = word_to_masm_push_string(&key2_final_value),
        key3_value = word_to_masm_push_string(&key3_final_value),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .tx_script(tx_script)
        .build()
        .execute()
        .context("failed to execute transaction")?;
    let maps_delta = executed_tx.account_delta().storage().maps();

    let mut map0_delta =
        maps_delta.get(&0).expect("delta for map 0 should exist").clone().into_map();
    let mut map1_delta =
        maps_delta.get(&1).expect("delta for map 1 should exist").clone().into_map();

    assert_eq!(map0_delta.len(), 2);
    assert_eq!(map0_delta.remove(&LexicographicWord::new(key0)).unwrap(), key0_final_value);
    assert_eq!(map0_delta.remove(&LexicographicWord::new(key1)).unwrap(), key1_final_value);

    assert_eq!(map1_delta.len(), 1);
    assert_eq!(map1_delta.remove(&LexicographicWord::new(key3)).unwrap(), key3_final_value);

    validate_account_delta(&executed_tx).context("failed to validate delta")?;

    Ok(())
}

/// Tests that increasing, decreasing the amount of a fungible asset results in the correct delta.
/// - Asset0 is increased by 100 and decreased by 200 -> Delta: -100.
/// - Asset1 is increased by 100 and decreased by 100 -> Delta: 0.
/// - Asset2 is increased by 200 and decreased by 100 -> Delta: 100.
/// - Asset3 is decreased by [`FungibleAsset::MAX_AMOUNT`] -> Delta: -MAX_AMOUNT.
/// - Asset4 is increased by [`FungibleAsset::MAX_AMOUNT`] -> Delta: MAX_AMOUNT.
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
    let faucet3: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .build_with_seed(rand::random());
    let faucet4: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .build_with_seed(rand::random());

    let original_asset0 = FungibleAsset::new(faucet0, 300)?;
    let original_asset1 = FungibleAsset::new(faucet1, 200)?;
    let original_asset2 = FungibleAsset::new(faucet2, 100)?;
    let original_asset3 = FungibleAsset::new(faucet3, FungibleAsset::MAX_AMOUNT)?;

    let added_asset0 = FungibleAsset::new(faucet0, 100)?;
    let added_asset1 = FungibleAsset::new(faucet1, 100)?;
    let added_asset2 = FungibleAsset::new(faucet2, 200)?;
    let added_asset4 = FungibleAsset::new(faucet4, FungibleAsset::MAX_AMOUNT)?;

    let removed_asset0 = FungibleAsset::new(faucet0, 200)?;
    let removed_asset1 = FungibleAsset::new(faucet1, 100)?;
    let removed_asset2 = FungibleAsset::new(faucet2, 100)?;
    let removed_asset3 = FungibleAsset::new(faucet3, FungibleAsset::MAX_AMOUNT)?;

    let TestSetup { mut mock_chain, account_id } = setup_asset_test(
        [original_asset0, original_asset1, original_asset2, original_asset3].map(Asset::from),
    );

    let mut added_notes = vec![];
    for added_asset in [added_asset0, added_asset1, added_asset2, added_asset4] {
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
        push.{asset3} exec.create_note_with_asset
        # => []

        # nonce must increase for state changing transactions
        push.1 exec.incr_nonce
    end
    ",
        asset0 = word_to_masm_push_string(&removed_asset0.into()),
        asset1 = word_to_masm_push_string(&removed_asset1.into()),
        asset2 = word_to_masm_push_string(&removed_asset2.into()),
        asset3 = word_to_masm_push_string(&removed_asset3.into()),
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

    assert_eq!(added_assets.len(), 2);
    assert_eq!(removed_assets.len(), 2);

    assert_eq!(
        added_assets.remove(&Digest::from(original_asset2.vault_key())).unwrap(),
        added_asset2.amount() - removed_asset2.amount()
    );
    assert_eq!(
        added_assets.remove(&Digest::from(added_asset4.vault_key())).unwrap(),
        added_asset4.amount()
    );

    assert_eq!(
        removed_assets.remove(&Digest::from(original_asset0.vault_key())).unwrap(),
        removed_asset0.amount() - added_asset0.amount()
    );
    assert_eq!(
        removed_assets.remove(&Digest::from(original_asset3.vault_key())).unwrap(),
        removed_asset3.amount()
    );

    validate_account_delta(&executed_tx)?;

    Ok(())
}

/// Tests that adding, removing non-fungible assets results in the correct delta.
/// - Asset0 is added to the vault -> Delta: Add.
/// - Asset1 is removed from the vault -> Delta: Remove.
/// - Asset2 is added and removed -> Delta: No Change.
/// - Asset3 is removed and added -> Delta: No Change.
#[test]
fn non_fungible_asset_delta() -> anyhow::Result<()> {
    let mut rng = rand::rng();
    // Test with random IDs to make sure the ordering in the MASM and Rust implementations
    // matches.
    let faucet0: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::NonFungibleFaucet)
        .build_with_seed(rng.random());
    let faucet1: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::NonFungibleFaucet)
        .build_with_seed(rng.random());
    let faucet2: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::NonFungibleFaucet)
        .build_with_seed(rng.random());
    let faucet3: AccountId = AccountIdBuilder::new()
        .account_type(AccountType::NonFungibleFaucet)
        .build_with_seed(rng.random());

    let asset0 = NonFungibleAssetBuilder::new(faucet0.prefix(), &mut rng)?.build()?;
    let asset1 = NonFungibleAssetBuilder::new(faucet1.prefix(), &mut rng)?.build()?;
    let asset2 = NonFungibleAssetBuilder::new(faucet2.prefix(), &mut rng)?.build()?;
    let asset3 = NonFungibleAssetBuilder::new(faucet3.prefix(), &mut rng)?.build()?;

    let TestSetup { mut mock_chain, account_id } =
        setup_asset_test([asset1, asset3].map(Asset::from));

    let mut added_notes = vec![];
    for added_asset in [asset0, asset2] {
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
        push.{asset1} exec.create_note_with_asset
        # => []
        push.{asset2} exec.create_note_with_asset
        # => []

        # remove and re-add asset 3
        push.{asset3}
        exec.remove_asset
        # => [ASSET]
        exec.add_asset dropw
        # => []

        # nonce must increase for state changing transactions
        push.1 exec.incr_nonce
    end
    ",
        asset1 = word_to_masm_push_string(&asset1.into()),
        asset2 = word_to_masm_push_string(&asset2.into()),
        asset3 = word_to_masm_push_string(&asset3.into()),
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
        .map(|asset| (Digest::from(asset.vault_key()), asset.unwrap_non_fungible()))
        .collect::<BTreeMap<_, _>>();
    let mut removed_assets = executed_tx
        .account_delta()
        .vault()
        .removed_assets()
        .map(|asset| (Digest::from(asset.vault_key()), asset.unwrap_non_fungible()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(added_assets.len(), 1);
    assert_eq!(removed_assets.len(), 1);

    assert_eq!(added_assets.remove(&Digest::from(asset0.vault_key())).unwrap(), asset0);
    assert_eq!(removed_assets.remove(&Digest::from(asset1.vault_key())).unwrap(), asset1);

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

      #! Inputs:  [index, KEY, VALUE]
      #! Outputs: []
      proc.set_map_item
          repeat.7 push.0 movdn.9 end
          # => [index, KEY, VALUE, pad(7)]

          call.account::set_map_item
          # => [OLD_MAP_ROOT, OLD_MAP_VALUE, pad(8)]

          dropw dropw dropw dropw
          # => []
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

      #! Inputs:  [ASSET]
      #! Outputs: [ASSET']
      proc.add_asset
          repeat.12 push.0 movdn.4 end
          # => [ASSET, pad(12)]

          call.account::add_asset
          # => [ASSET', pad(12)]

          repeat.12 movup.4 drop end
          # => [ASSET']
      end

      #! Inputs:  [ASSET]
      #! Outputs: [ASSET]
      proc.remove_asset
          repeat.12 push.0 movdn.4 end
          # => [ASSET, pad(12)]

          call.account::remove_asset
          # => [ASSET, pad(12)]

          repeat.12 movup.4 drop end
          # => [ASSET]
      end
";
