use alloc::vec::Vec;
use std::collections::BTreeMap;
use std::string::String;

use anyhow::Context;
use miden_lib::testing::account_component::AccountMockComponent;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    AccountBuilder,
    AccountId,
    AccountStorage,
    AccountType,
    StorageMap,
    StorageSlot,
};
use miden_objects::asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset};
use miden_objects::note::{Note, NoteExecutionHint, NoteTag, NoteType};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    ACCOUNT_ID_SENDER,
    AccountIdBuilder,
};
use miden_objects::testing::asset::NonFungibleAssetBuilder;
use miden_objects::testing::constants::{
    CONSUMED_ASSET_1_AMOUNT,
    CONSUMED_ASSET_3_AMOUNT,
    FUNGIBLE_ASSET_AMOUNT,
    NON_FUNGIBLE_ASSET_DATA,
    NON_FUNGIBLE_ASSET_DATA_2,
};
use miden_objects::testing::storage::{STORAGE_INDEX_0, STORAGE_INDEX_2};
use miden_objects::transaction::TransactionScript;
use miden_objects::{EMPTY_WORD, Felt, LexicographicWord, Word, ZERO};
use miden_tx::utils::word_to_masm_push_string;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use winter_rand_utils::rand_value;

use crate::utils::create_p2any_note;
use crate::{Auth, MockChain, TransactionContextBuilder};

// ACCOUNT DELTA TESTS
//
// Note that in all of these tests, the transaction executor will ensure that the account delta
// commitment computed in-kernel and in the host match.
// ================================================================================================

/// Tests that a noop transaction with [`Auth::Noop`] results in an empty nonce delta with an empty
/// word as its commitment.
///
/// In order to make the account delta empty but the transaction still legal, we consume a note
/// without assets.
#[test]
fn empty_account_delta_commitment_is_empty_word() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account(Auth::Noop)?;
    let p2any_note =
        builder.add_p2any_note(AccountId::try_from(ACCOUNT_ID_SENDER).unwrap(), &[])?;
    let mock_chain = builder.build()?;

    let executed_tx = mock_chain
        .build_tx_context(account.id(), &[p2any_note.id()], &[])
        .expect("failed to build tx context")
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce_delta(), ZERO);
    assert!(executed_tx.account_delta().is_empty());
    assert_eq!(executed_tx.account_delta().to_commitment(), Word::empty());

    Ok(())
}

/// Tests that a noop transaction with [`Auth::IncrNonce`] results in a nonce delta of 1.
#[test]
fn delta_nonce() -> anyhow::Result<()> {
    let TestSetup { mock_chain, account_id } = setup_test([], [])?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .expect("failed to build tx context")
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce_delta(), Felt::new(1));

    Ok(())
}

/// Tests that setting new values for value storage slots results in the correct delta.
///
/// - Slot 0: [2,4,6,8]  -> [3,4,5,6] -> EMPTY_WORD -> Delta: EMPTY_WORD
/// - Slot 1: EMPTY_WORD -> [3,4,5,6]               -> Delta: [3,4,5,6]
/// - Slot 2: [1,3,5,7]  -> [1,3,5,7]               -> Delta: None
/// - Slot 3: [1,3,5,7]  -> [2,3,4,5] -> [1,3,5,7]  -> Delta: None
#[test]
fn storage_delta_for_value_slots() -> anyhow::Result<()> {
    let slot_0_init_value = Word::from([2, 4, 6, 8u32]);
    let slot_0_tmp_value = Word::from([3, 4, 5, 6u32]);
    let slot_0_final_value = EMPTY_WORD;

    let slot_1_init_value = EMPTY_WORD;
    let slot_1_final_value = Word::from([3, 4, 5, 6u32]);

    let slot_2_init_value = Word::from([1, 3, 5, 7u32]);
    let slot_2_final_value = slot_2_init_value;

    let slot_3_init_value = Word::from([1, 3, 5, 7u32]);
    let slot_3_tmp_value = Word::from([2, 3, 4, 5u32]);
    let slot_3_final_value = slot_3_init_value;

    let TestSetup { mock_chain, account_id } = setup_test(
        vec![
            StorageSlot::Value(slot_0_init_value),
            StorageSlot::Value(slot_1_init_value),
            StorageSlot::Value(slot_2_init_value),
            StorageSlot::Value(slot_3_init_value),
        ],
        [],
    )?;

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

          push.{tmp_slot_3_value}
          push.3
          # => [index, VALUE]
          exec.set_item
          # => []

          push.{final_slot_3_value}
          push.3
          # => [index, VALUE]
          exec.set_item
          # => []
      end
      ",
        // Set slot 0 to some other value initially.
        tmp_slot_0_value = word_to_masm_push_string(&slot_0_tmp_value),
        final_slot_0_value = word_to_masm_push_string(&slot_0_final_value),
        final_slot_1_value = word_to_masm_push_string(&slot_1_final_value),
        final_slot_2_value = word_to_masm_push_string(&slot_2_final_value),
        tmp_slot_3_value = word_to_masm_push_string(&slot_3_tmp_value),
        final_slot_3_value = word_to_masm_push_string(&slot_3_final_value),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])
        .expect("failed to build tx context")
        .tx_script(tx_script)
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;

    let storage_values_delta = executed_tx
        .account_delta()
        .storage()
        .values()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect::<Vec<_>>();

    // Note that slots 2 and 3 are absent because their values haven't effectively changed.
    assert_eq!(storage_values_delta, &[(0u8, slot_0_final_value), (1u8, slot_1_final_value)]);

    Ok(())
}

/// Tests that setting new values for map storage slots results in the correct delta.
///
/// - Slot 0: key0: EMPTY_WORD -> [1,2,3,4]              -> Delta: [1,2,3,4]
/// - Slot 0: key1: EMPTY_WORD -> [1,2,3,4] -> [2,3,4,5] -> Delta: [2,3,4,5]
/// - Slot 1: key2: [1,2,3,4]  -> [1,2,3,4]              -> Delta: None
/// - Slot 1: key3: [1,2,3,4]  -> EMPTY_WORD             -> Delta: EMPTY_WORD
/// - Slot 1: key4: [1,2,3,4]  -> [2,3,4,5] -> [1,2,3,4] -> Delta: None
/// - Slot 2: key5: [1,2,3,4]  -> [2,3,4,5] -> [1,2,3,4] -> Delta: None
///   - key5 and key4 are the same scenario, but in different slots. In particular, slot 2's delta
///     map will be empty after normalization and so it shouldn't be present in the delta at all.
#[test]
fn storage_delta_for_map_slots() -> anyhow::Result<()> {
    // Test with random keys to make sure the ordering in the MASM and Rust implementations
    // matches.
    let key0 = rand_value::<Word>();
    let key1 = rand_value::<Word>();
    let key2 = rand_value::<Word>();
    let key3 = rand_value::<Word>();
    let key4 = rand_value::<Word>();
    let key5 = rand_value::<Word>();

    let key0_init_value = EMPTY_WORD;
    let key1_init_value = EMPTY_WORD;
    let key2_init_value = Word::from([1, 2, 3, 4u32]);
    let key3_init_value = Word::from([1, 2, 3, 4u32]);
    let key4_init_value = Word::from([1, 2, 3, 4u32]);
    let key5_init_value = Word::from([1, 2, 3, 4u32]);

    let key0_final_value = Word::from([1, 2, 3, 4u32]);
    let key1_tmp_value = Word::from([1, 2, 3, 4u32]);
    let key1_final_value = Word::from([2, 3, 4, 5u32]);
    let key2_final_value = key2_init_value;
    let key3_final_value = EMPTY_WORD;
    let key4_tmp_value = Word::from([2, 3, 4, 5u32]);
    let key4_final_value = Word::from([1, 2, 3, 4u32]);
    let key5_tmp_value = Word::from([2, 3, 4, 5u32]);
    let key5_final_value = Word::from([1, 2, 3, 4u32]);

    let mut map0 = StorageMap::new();
    map0.insert(key0, key0_init_value);
    map0.insert(key1, key1_init_value);

    let mut map1 = StorageMap::new();
    map1.insert(key2, key2_init_value);
    map1.insert(key3, key3_init_value);
    map1.insert(key4, key4_init_value);

    let mut map2 = StorageMap::new();
    map2.insert(key5, key5_init_value);

    let TestSetup { mock_chain, account_id } = setup_test(
        vec![
            StorageSlot::Map(map0),
            StorageSlot::Map(map1),
            StorageSlot::Map(map2),
            // Include an empty map which does not receive any updates, to test that the "metadata
            // header" in the delta commitment is not appended if there are no updates to a map
            // slot.
            StorageSlot::Map(StorageMap::new()),
        ],
        [],
    )?;

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

          push.{key4_tmp_value}.{key4}.1
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key4_value}.{key4}.1
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key5_tmp_value}.{key5}.2
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []

          push.{key5_value}.{key5}.2
          # => [index, KEY, VALUE]
          exec.set_map_item
          # => []
      end
      ",
        key0 = word_to_masm_push_string(&key0),
        key1 = word_to_masm_push_string(&key1),
        key2 = word_to_masm_push_string(&key2),
        key3 = word_to_masm_push_string(&key3),
        key4 = word_to_masm_push_string(&key4),
        key5 = word_to_masm_push_string(&key5),
        key0_value = word_to_masm_push_string(&key0_final_value),
        key1_tmp_value = word_to_masm_push_string(&key1_tmp_value),
        key1_value = word_to_masm_push_string(&key1_final_value),
        key2_value = word_to_masm_push_string(&key2_final_value),
        key3_value = word_to_masm_push_string(&key3_final_value),
        key4_tmp_value = word_to_masm_push_string(&key4_tmp_value),
        key4_value = word_to_masm_push_string(&key4_final_value),
        key5_tmp_value = word_to_masm_push_string(&key5_tmp_value),
        key5_value = word_to_masm_push_string(&key5_final_value),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &[], &[])?
        .tx_script(tx_script)
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;
    let maps_delta = executed_tx.account_delta().storage().maps();

    // Note that there should be no delta for map2 since it was normalized to an empty map which
    // should be removed.
    assert_eq!(maps_delta.len(), 2);
    assert!(maps_delta.get(&2).is_none(), "map2 should not have a delta");

    let mut map0_delta =
        maps_delta.get(&0).expect("delta for map 0 should exist").clone().into_map();
    let mut map1_delta =
        maps_delta.get(&1).expect("delta for map 1 should exist").clone().into_map();

    assert_eq!(map0_delta.len(), 2);
    assert_eq!(map0_delta.remove(&LexicographicWord::new(key0)).unwrap(), key0_final_value);
    assert_eq!(map0_delta.remove(&LexicographicWord::new(key1)).unwrap(), key1_final_value);

    assert_eq!(map1_delta.len(), 1);
    assert_eq!(map1_delta.remove(&LexicographicWord::new(key3)).unwrap(), key3_final_value);

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

    let TestSetup { mut mock_chain, account_id } = setup_test(
        [],
        [original_asset0, original_asset1, original_asset2, original_asset3].map(Asset::from),
    )?;

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
    mock_chain.prove_next_block()?;

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
    end
    ",
        asset0 = word_to_masm_push_string(&removed_asset0.into()),
        asset1 = word_to_masm_push_string(&removed_asset1.into()),
        asset2 = word_to_masm_push_string(&removed_asset2.into()),
        asset3 = word_to_masm_push_string(&removed_asset3.into()),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &added_notes.iter().map(Note::id).collect::<Vec<_>>(), &[])?
        .tx_script(tx_script)
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;

    let mut added_assets = executed_tx
        .account_delta()
        .vault()
        .added_assets()
        .map(|asset| (asset.unwrap_fungible().faucet_id(), asset.unwrap_fungible().amount()))
        .collect::<BTreeMap<_, _>>();
    let mut removed_assets = executed_tx
        .account_delta()
        .vault()
        .removed_assets()
        .map(|asset| (asset.unwrap_fungible().faucet_id(), asset.unwrap_fungible().amount()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(added_assets.len(), 2);
    assert_eq!(removed_assets.len(), 2);

    assert_eq!(
        added_assets.remove(&original_asset2.faucet_id()).unwrap(),
        added_asset2.amount() - removed_asset2.amount()
    );
    assert_eq!(added_assets.remove(&added_asset4.faucet_id()).unwrap(), added_asset4.amount());

    assert_eq!(
        removed_assets.remove(&original_asset0.faucet_id()).unwrap(),
        removed_asset0.amount() - added_asset0.amount()
    );
    assert_eq!(
        removed_assets.remove(&original_asset3.faucet_id()).unwrap(),
        removed_asset3.amount()
    );

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
        setup_test([], [asset1, asset3].map(Asset::from))?;

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
    mock_chain.prove_next_block()?;

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
    end
    ",
        asset1 = word_to_masm_push_string(&asset1.into()),
        asset2 = word_to_masm_push_string(&asset2.into()),
        asset3 = word_to_masm_push_string(&asset3.into()),
    ))?;

    let executed_tx = mock_chain
        .build_tx_context(account_id, &added_notes.iter().map(Note::id).collect::<Vec<_>>(), &[])?
        .tx_script(tx_script)
        .build()?
        .execute_blocking()
        .context("failed to execute transaction")?;

    let mut added_assets = executed_tx
        .account_delta()
        .vault()
        .added_assets()
        .map(|asset| (asset.faucet_id_prefix(), asset.unwrap_non_fungible()))
        .collect::<BTreeMap<_, _>>();
    let mut removed_assets = executed_tx
        .account_delta()
        .vault()
        .removed_assets()
        .map(|asset| (asset.faucet_id_prefix(), asset.unwrap_non_fungible()))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(added_assets.len(), 1);
    assert_eq!(removed_assets.len(), 1);

    assert_eq!(added_assets.remove(&asset0.faucet_id_prefix()).unwrap(), asset0);
    assert_eq!(removed_assets.remove(&asset1.faucet_id_prefix()).unwrap(), asset1);

    Ok(())
}

/// Tests that adding and removing assets and updating value and map storage slots results in the
/// correct delta.
#[test]
fn asset_and_storage_delta() -> anyhow::Result<()> {
    let account_assets = AssetVault::mock().assets().collect::<Vec<Asset>>();

    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(AccountMockComponent::new_with_slots(AccountStorage::mock_storage_slots())?)
        .with_assets(account_assets)
        .build_existing()?;

    // updated storage
    let updated_slot_value = Word::from([7, 9, 11, 13u32]);

    // updated storage map
    let updated_map_key = Word::from([14, 15, 16, 17u32]);
    let updated_map_value = Word::from([18, 19, 20, 21u32]);

    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let removed_assets = [removed_asset_1, removed_asset_2, removed_asset_3];

    let tag1 =
        NoteTag::from_account_id(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?);
    let tag2 = NoteTag::for_local_use_case(0, 0)?;
    let tag3 = NoteTag::for_local_use_case(0, 0)?;
    let tags = [tag1, tag2, tag3];

    let aux_array = [Felt::new(27), Felt::new(28), Felt::new(29)];

    let note_types = [NoteType::Private; 3];

    tag1.validate(NoteType::Private)
        .expect("note tag 1 should support private notes");
    tag2.validate(NoteType::Private)
        .expect("note tag 2 should support private notes");
    tag3.validate(NoteType::Private)
        .expect("note tag 3 should support private notes");

    let execution_hint_1 = Felt::from(NoteExecutionHint::always());
    let execution_hint_2 = Felt::from(NoteExecutionHint::none());
    let execution_hint_3 = Felt::from(NoteExecutionHint::on_block_slot(1, 1, 1));
    let hints = [execution_hint_1, execution_hint_2, execution_hint_3];

    let mut send_asset_script = String::new();
    for i in 0..3 {
        send_asset_script.push_str(&format!(
            "
            ### note {i}
            # prepare the stack for a new note creation
            push.0.1.2.3           # recipient
            push.{EXECUTION_HINT}  # note_execution_hint
            push.{NOTETYPE}        # note_type
            push.{aux}             # aux
            push.{tag}             # tag
            # => [tag, aux, note_type, execution_hint, RECIPIENT]

            # pad the stack before calling the `create_note`
            padw padw swapdw
            # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8)]

            # create the note
            call.tx::create_note
            # => [note_idx, pad(15)]

            # move an asset to the created note to partially deplete fungible asset balance
            swapw dropw push.{REMOVED_ASSET}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            # => [ASSET, note_idx, pad(11)]

            # clear the stack
            dropw dropw dropw dropw
        ",
            EXECUTION_HINT = hints[i],
            NOTETYPE = note_types[i] as u8,
            aux = aux_array[i],
            tag = tags[i],
            REMOVED_ASSET = word_to_masm_push_string(&Word::from(removed_assets[i]))
        ));
    }

    let tx_script_src = format!(
        "\
        use.test::account
        use.miden::tx

        ## TRANSACTION SCRIPT
        ## ========================================================================================
        begin
            ## Update account storage item
            ## ------------------------------------------------------------------------------------
            # push a new value for the storage slot onto the stack
            push.{UPDATED_SLOT_VALUE}
            # => [13, 11, 9, 7]

            # get the index of account storage slot
            push.{STORAGE_INDEX_0}
            # => [idx, 13, 11, 9, 7]
            # update the storage value
            call.account::set_item dropw
            # => []

            ## Update account storage map
            ## ------------------------------------------------------------------------------------
            # push a new VALUE for the storage map onto the stack
            push.{UPDATED_MAP_VALUE}
            # => [18, 19, 20, 21]

            # push a new KEY for the storage map onto the stack
            push.{UPDATED_MAP_KEY}
            # => [14, 15, 16, 17, 18, 19, 20, 21]

            # get the index of account storage slot
            push.{STORAGE_INDEX_2}
            # => [idx, 14, 15, 16, 17, 18, 19, 20, 21]

            # update the storage value
            call.account::set_map_item dropw dropw dropw
            # => []

            ## Send some assets from the account vault
            ## ------------------------------------------------------------------------------------
            {send_asset_script}

            dropw dropw dropw dropw
        end
    ",
        UPDATED_SLOT_VALUE = word_to_masm_push_string(&updated_slot_value),
        UPDATED_MAP_VALUE = word_to_masm_push_string(&updated_map_value),
        UPDATED_MAP_KEY = word_to_masm_push_string(&updated_map_key),
    );

    let tx_script = ScriptBuilder::with_mock_account_library()?.compile_tx_script(tx_script_src)?;

    // Create the input note that carries the assets that we will assert later
    let input_note = {
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1)?;
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3)?;

        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT)?.into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT)?.into();
        let nonfungible_asset_1: Asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);

        create_p2any_note(account.id(), &[fungible_asset_1, fungible_asset_3, nonfungible_asset_1])
    };

    let tx_context = TransactionContextBuilder::new(account)
        .extend_input_notes(vec![input_note.clone()])
        .tx_script(tx_script)
        .build()?;

    // Storing assets that will be added to assert correctness later
    let added_assets = input_note.assets().iter().cloned().collect::<Vec<_>>();

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness
    let executed_transaction = tx_context.execute_blocking()?;

    // nonce delta
    // --------------------------------------------------------------------------------------------

    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    // storage delta
    // --------------------------------------------------------------------------------------------
    // We expect one updated item and one updated map
    assert_eq!(executed_transaction.account_delta().storage().values().len(), 1);
    assert_eq!(
        executed_transaction.account_delta().storage().values().get(&STORAGE_INDEX_0),
        Some(&updated_slot_value)
    );

    assert_eq!(executed_transaction.account_delta().storage().maps().len(), 1);
    let map_delta = executed_transaction
        .account_delta()
        .storage()
        .maps()
        .get(&STORAGE_INDEX_2)
        .context("failed to get expected value from storage map")?
        .entries();
    assert_eq!(
        *map_delta.get(&LexicographicWord::new(updated_map_key)).unwrap(),
        updated_map_value
    );

    // vault delta
    // --------------------------------------------------------------------------------------------
    // assert that added assets are tracked
    assert!(
        executed_transaction
            .account_delta()
            .vault()
            .added_assets()
            .all(|x| added_assets.contains(&x))
    );
    assert_eq!(
        added_assets.len(),
        executed_transaction.account_delta().vault().added_assets().count()
    );

    // assert that removed assets are tracked
    assert!(
        executed_transaction
            .account_delta()
            .vault()
            .removed_assets()
            .all(|x| removed_assets.contains(&x))
    );
    assert_eq!(
        removed_assets.len(),
        executed_transaction.account_delta().vault().removed_assets().count()
    );
    Ok(())
}

/// Tests that adding a fungible asset with amount zero to the account vault works and does not
/// result in an account delta entry.
#[test]
fn adding_amount_zero_fungible_asset_to_account_vault_works() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account(Auth::IncrNonce)?;
    let input_note = builder.add_p2id_note(
        account.id(),
        account.id(),
        &[FungibleAsset::mock(0)],
        NoteType::Private,
    )?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account, &[input_note.id()], &[])?
        .build()?
        .execute_blocking()?;

    assert!(tx.account_delta().vault().is_empty());

    Ok(())
}

// TEST HELPERS
// ================================================================================================

struct TestSetup {
    mock_chain: MockChain,
    account_id: AccountId,
}

fn setup_test(
    storage_slots: impl IntoIterator<Item = StorageSlot>,
    assets: impl IntoIterator<Item = Asset>,
) -> anyhow::Result<TestSetup> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account_with_storage_and_assets(
        Auth::IncrNonce,
        storage_slots,
        assets,
    )?;

    let mock_chain = builder.build()?;

    Ok(TestSetup { mock_chain, account_id: account.id() })
}

fn compile_tx_script(code: impl AsRef<str>) -> anyhow::Result<TransactionScript> {
    let code = format!(
        "
    {TEST_ACCOUNT_CONVENIENCE_WRAPPERS}
    {code}
    ",
        code = code.as_ref()
    );

    ScriptBuilder::with_mock_account_library()?
        .compile_tx_script(&code)
        .context("failed to compile tx script")
}

const TEST_ACCOUNT_CONVENIENCE_WRAPPERS: &str = "
      use.test::account
      use.miden::tx

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

          call.tx::create_note
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
