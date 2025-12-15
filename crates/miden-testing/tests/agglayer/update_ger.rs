extern crate alloc;

use miden_lib::agglayer::{bridge_in_component, update_ger_script};
use miden_objects::account::{Account, AccountStorageMode, StorageSlot, StorageSlotName};
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};
use miden_objects::{Felt, LexicographicWord, Word};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests the UPDATE_GER flow: UPDATE_GER note -> Bridge In account -> Global Exit Root updated.
#[tokio::test]
async fn test_update_ger_flow() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create bridge operator account
    let bridge_operator = builder.add_existing_wallet(Auth::IncrNonce)?;

    // Create bridge account with bridge_in component for GER updates
    // The bridge_in_component function expects storage slots to be provided
    let ger_storage_slot_name = StorageSlotName::new("miden::agglayer::GER").unwrap();
    let bridge_operator_slot_name =
        StorageSlotName::new("miden::agglayer::bridge_operator").unwrap();

    // Store the bridge operator account ID in the bridge operator slot
    let operator_id_word = Word::from([
        Felt::new(0),
        Felt::new(0),
        Felt::new(bridge_operator.id().suffix().as_int()),
        bridge_operator.id().prefix().as_felt(),
    ]);

    let bridge_storage_slots = vec![
        StorageSlot::with_empty_map(ger_storage_slot_name.clone()),
        StorageSlot::with_value(bridge_operator_slot_name, operator_id_word),
    ];
    let bridge_component = bridge_in_component(bridge_storage_slots);
    let bridge_account_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(bridge_component);
    let bridge_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        bridge_account_builder,
        AccountState::Exists,
    )?;

    // Build mock chain
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // Generate random 32-byte GER value (8 u32 values) and 32-bit GER index (1 u32 value)
    let mut rng = rand::rng();
    let ger_value_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index = rng.random::<u32>();

    // Create note inputs (9 u32 values total)
    let mut input_values = Vec::new();
    for &value in &ger_value_u32s {
        input_values.push(Felt::new(value as u64));
    }
    input_values.push(Felt::new(ger_index as u64));

    println!("input values: {:?}", input_values);
    println!("ger index: {}", ger_index);

    // Create UPDATE_GER note from the bridge operator
    let inputs = NoteInputs::new(input_values)?;
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let note_metadata = NoteMetadata::new(
        bridge_operator.id(), // Note sender is the bridge operator
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;
    let note_assets = NoteAssets::new(vec![])?;
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let note_recipient = NoteRecipient::new(serial_num, update_ger_script(), inputs);
    let update_ger_note = Note::new(note_assets, note_metadata, note_recipient);

    // Execute UPDATE_GER note against bridge account
    let tx_context = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note])?
        .build()?;
    let executed_transaction = tx_context.execute().await?;

    // Verify GER was updated in bridge account storage
    let account_delta = executed_transaction.account_delta();
    let storage_maps = account_delta.storage().maps();
    let map_delta = storage_maps
        .get(&ger_storage_slot_name)
        .expect("GER storage map delta should exist");
    let entries = map_delta.entries();

    // Verify GER_UPPER at key [0,0,0,0] (contains ger_value_u32s[0..4])
    let key_upper = Word::from([Felt::new(0), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let lex_key_upper = LexicographicWord::new(key_upper);
    let value_upper = entries.get(&lex_key_upper).expect("GER upper word should be stored");
    let expected_word_upper = Word::from([
        Felt::new(ger_value_u32s[3] as u64),
        Felt::new(ger_value_u32s[2] as u64),
        Felt::new(ger_value_u32s[1] as u64),
        Felt::new(ger_value_u32s[0] as u64),
    ]);
    assert_eq!(*value_upper, expected_word_upper);

    // Verify GER_LOWER at key [0,0,0,1] (contains ger_value_u32s[4..8])
    let key_lower = Word::from([Felt::new(1), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let lex_key_lower = LexicographicWord::new(key_lower);
    let value_lower = entries.get(&lex_key_lower).expect("GER lower word should be stored");
    let expected_word_lower = Word::from([
        Felt::new(ger_value_u32s[7] as u64),
        Felt::new(ger_value_u32s[6] as u64),
        Felt::new(ger_value_u32s[5] as u64),
        Felt::new(ger_value_u32s[4] as u64),
    ]);
    assert_eq!(*value_lower, expected_word_lower);

    Ok(())
}

/// Tests that UPDATE_GER fails when called by unauthorized account
#[tokio::test]
async fn test_update_ger_unauthorized_fails() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create bridge operator account
    let bridge_operator = builder.add_existing_wallet(Auth::IncrNonce)?;

    // Create unauthorized account
    let unauthorized_account = builder.add_existing_wallet(Auth::IncrNonce)?;

    // Create bridge account with bridge_in component for GER updates
    let ger_storage_slot_name = StorageSlotName::new("miden::agglayer::GER").unwrap();
    let bridge_operator_slot_name =
        StorageSlotName::new("miden::agglayer::bridge_operator").unwrap();

    // Store the bridge operator account ID in the bridge operator slot
    let operator_id_word = Word::from([
        Felt::new(0),
        Felt::new(0),
        Felt::new(bridge_operator.id().suffix().as_int()),
        bridge_operator.id().prefix().as_felt(),
    ]);

    let bridge_storage_slots = vec![
        StorageSlot::with_empty_map(ger_storage_slot_name.clone()),
        StorageSlot::with_value(bridge_operator_slot_name, operator_id_word),
    ];
    let bridge_component = bridge_in_component(bridge_storage_slots);
    let bridge_account_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(bridge_component);
    let bridge_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        bridge_account_builder,
        AccountState::Exists,
    )?;

    // Build mock chain
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // Generate random 32-byte GER value (8 u32 values) and 32-bit GER index (1 u32 value)
    let mut rng = rand::rng();
    let ger_value_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index = rng.random::<u32>();

    // Create note inputs (9 u32 values total)
    let mut input_values = Vec::new();
    for &value in &ger_value_u32s {
        input_values.push(Felt::new(value as u64));
    }
    input_values.push(Felt::new(ger_index as u64));

    // Create UPDATE_GER note from unauthorized account (should fail)
    let inputs = NoteInputs::new(input_values)?;
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let note_metadata = NoteMetadata::new(
        unauthorized_account.id(), // Note sender is NOT the bridge operator
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;
    let note_assets = NoteAssets::new(vec![])?;
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let note_recipient = NoteRecipient::new(serial_num, update_ger_script(), inputs);
    let update_ger_note = Note::new(note_assets, note_metadata, note_recipient);

    // Execute UPDATE_GER note against bridge account - should fail
    let tx_context = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note])?
        .build()?;
    let result = tx_context.execute().await;

    // Verify that the transaction failed due to authorization
    assert!(result.is_err(), "Transaction should fail when called by unauthorized account");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("note sender is not the authorized bridge operator"),
        "Error should mention authorization failure, got: {}",
        error_msg
    );

    Ok(())
}
