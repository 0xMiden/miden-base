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
use miden_objects::transaction::OutputNote;
use miden_objects::{Felt, LexicographicWord, Word};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

// Test utilities and common setup functions
// ================================================================================================

/// Common test setup for UPDATE_GER tests
struct UpdateGerTestSetup {
    pub bridge_operator: Account,
    pub bridge_account: Account,
    pub ger_storage_slot_name: StorageSlotName,
    pub mock_chain: MockChain,
}

impl UpdateGerTestSetup {
    /// Creates a new test setup with bridge operator and bridge account
    fn new() -> anyhow::Result<Self> {
        let mut builder = MockChain::builder();

        // Create bridge operator account
        let bridge_operator = builder.add_existing_wallet(Auth::IncrNonce)?;

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

        let mock_chain = builder.build()?;

        Ok(Self {
            bridge_operator,
            bridge_account,
            ger_storage_slot_name,
            mock_chain,
        })
    }
}

/// Creates an UPDATE_GER note with the given parameters
fn create_update_ger_note(
    sender_id: miden_objects::account::AccountId,
    ger_values: [u32; 8],
    ger_index: u32,
    serial_num: Word,
) -> anyhow::Result<Note> {
    let mut input_values = Vec::new();
    for &value in &ger_values {
        input_values.push(Felt::new(value as u64));
    }
    input_values.push(Felt::new(ger_index as u64));

    let inputs = NoteInputs::new(input_values)?;
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let note_metadata = NoteMetadata::new(
        sender_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;
    let note_assets = NoteAssets::new(vec![])?;
    let note_recipient = NoteRecipient::new(serial_num, update_ger_script(), inputs);
    Ok(Note::new(note_assets, note_metadata, note_recipient))
}

/// Verifies that the GER index is stored correctly in the account storage
fn verify_ger_index_stored(
    account_delta: &miden_objects::account::AccountDelta,
    ger_storage_slot_name: &StorageSlotName,
    expected_ger_index: u32,
) -> anyhow::Result<()> {
    let storage_maps = account_delta.storage().maps();
    let map_delta = storage_maps
        .get(ger_storage_slot_name)
        .expect("GER storage map delta should exist");
    let entries = map_delta.entries();

    let key_index = Word::from([Felt::new(2), Felt::new(0), Felt::new(0), Felt::new(0)]);
    let lex_key_index = LexicographicWord::new(key_index);
    let value_index = entries.get(&lex_key_index).expect("GER index should be stored");
    let expected_word_index = Word::from([
        Felt::new(expected_ger_index as u64),
        Felt::new(0),
        Felt::new(0),
        Felt::new(0),
    ]);
    assert_eq!(*value_index, expected_word_index);
    Ok(())
}

/// Verifies that the GER values are stored correctly in the account storage
fn verify_ger_values_stored(
    account_delta: &miden_objects::account::AccountDelta,
    ger_storage_slot_name: &StorageSlotName,
    ger_value_u32s: [u32; 8],
) -> anyhow::Result<()> {
    let storage_maps = account_delta.storage().maps();
    let map_delta = storage_maps
        .get(ger_storage_slot_name)
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

/// Tests the UPDATE_GER flow: UPDATE_GER note -> Bridge In account -> Global Exit Root updated.
#[tokio::test]
async fn test_update_ger_flow() -> anyhow::Result<()> {
    let setup = UpdateGerTestSetup::new()?;
    let mut mock_chain = setup.mock_chain;
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
    let ger_index = 1u32; // Start with index 1 since storage starts at 0

    println!("ger index: {}", ger_index);

    // Create UPDATE_GER note from the bridge operator
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let update_ger_note =
        create_update_ger_note(setup.bridge_operator.id(), ger_value_u32s, ger_index, serial_num)?;

    // Execute UPDATE_GER note against bridge account
    let tx_context = mock_chain
        .build_tx_context(setup.bridge_account.id(), &[], &[update_ger_note])?
        .build()?;
    let executed_transaction = tx_context.execute().await?;

    // Verify GER values and index were updated in bridge account storage
    let account_delta = executed_transaction.account_delta();
    verify_ger_values_stored(&account_delta, &setup.ger_storage_slot_name, ger_value_u32s)?;
    verify_ger_index_stored(&account_delta, &setup.ger_storage_slot_name, ger_index)?;

    Ok(())
}

/// Tests that consuming two UPDATE_GER notes must be done monotonically
#[tokio::test]
async fn test_update_ger_monotonic_consumption() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create bridge operator account
    let bridge_operator = builder.add_existing_wallet(Auth::IncrNonce)?;

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

    // Generate first GER update with index 1
    let mut rng = rand::rng();
    let ger_value_1_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index_1 = 1u32; // First index should be 1 (current storage is 0, so 0+1=1)

    // Generate second GER update with index 2 (monotonic increase)
    let ger_value_2_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index_2 = 2u32; // Must be ger_index_1 + 1 (1+1=2)

    // Create both UPDATE_GER notes
    let serial_num_1 = Word::from([1, 2, 3, 4u32]);
    let update_ger_note_1 =
        create_update_ger_note(bridge_operator.id(), ger_value_1_u32s, ger_index_1, serial_num_1)?;

    let serial_num_2 = Word::from([5, 6, 7, 8u32]);
    let update_ger_note_2 =
        create_update_ger_note(bridge_operator.id(), ger_value_2_u32s, ger_index_2, serial_num_2)?;

    // Add both notes to the mock chain so they're available
    builder.add_output_note(OutputNote::Full(update_ger_note_1.clone()));
    builder.add_output_note(OutputNote::Full(update_ger_note_2.clone()));

    // Build mock chain
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // Execute first UPDATE_GER note
    let tx_context_1 = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note_1])?
        .build()?;
    let executed_transaction_1 = tx_context_1.execute().await?;

    // Verify first GER index was stored
    let account_delta_1 = executed_transaction_1.account_delta();
    verify_ger_index_stored(&account_delta_1, &ger_storage_slot_name, ger_index_1)?;

    // Create a new block after consuming the first note
    mock_chain.add_pending_executed_transaction(&executed_transaction_1)?;
    mock_chain.prove_next_block()?;

    // Execute second UPDATE_GER note in a separate transaction - should succeed
    let tx_context_2 = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note_2])?
        .build()?;
    let executed_transaction_2 = tx_context_2.execute().await?;

    // Verify second GER index was stored
    let account_delta_2 = executed_transaction_2.account_delta();
    verify_ger_index_stored(&account_delta_2, &ger_storage_slot_name, ger_index_2)?;

    Ok(())
}

/// Tests that consuming UPDATE_GER notes with non-monotonic indices fails
#[tokio::test]
async fn test_update_ger_non_monotonic_fails() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create bridge operator account
    let bridge_operator = builder.add_existing_wallet(Auth::IncrNonce)?;

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

    // Generate first GER update with index 1
    let mut rng = rand::rng();
    let ger_value_1_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index_1 = 1u32; // First index should be 1 (current storage is 0, so 0+1=1)

    // Generate second GER update with index 5 (non-monotonic - should be 2)
    let ger_value_2_u32s: [u32; 8] = [
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
        rng.random::<u32>(),
    ];
    let ger_index_2 = 5u32; // Non-monotonic jump (should be 2, but we're using 5)

    // Create both UPDATE_GER notes
    let serial_num_1 = Word::from([1, 2, 3, 4u32]);
    let update_ger_note_1 =
        create_update_ger_note(bridge_operator.id(), ger_value_1_u32s, ger_index_1, serial_num_1)?;

    let serial_num_2 = Word::from([5, 6, 7, 8u32]);
    let update_ger_note_2 =
        create_update_ger_note(bridge_operator.id(), ger_value_2_u32s, ger_index_2, serial_num_2)?;

    // Add both notes to the mock chain so they're available
    builder.add_output_note(OutputNote::Full(update_ger_note_1.clone()));
    builder.add_output_note(OutputNote::Full(update_ger_note_2.clone()));

    // Build mock chain
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // Execute first UPDATE_GER note
    let tx_context_1 = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note_1])?
        .build()?;
    let executed_transaction_1 = tx_context_1.execute().await?;

    // Update the mock chain with the new account state
    mock_chain.add_pending_executed_transaction(&executed_transaction_1)?;
    mock_chain.prove_next_block()?;

    // Execute second UPDATE_GER note - should fail
    let tx_context_2 = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note_2])?
        .build()?;
    let result = tx_context_2.execute().await;

    // Verify that the transaction failed due to non-monotonic index
    assert!(result.is_err(), "transaction should fail when GER index is not monotonic");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("ger index must increase monotonically by 1"),
        "error should mention monotonic failure, got: {}",
        error_msg
    );

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

    // Generate random GER data
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
    let ger_index = 1u32; // Start with index 1 since storage starts at 0

    // Create UPDATE_GER note from unauthorized account (should fail)
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let update_ger_note = create_update_ger_note(
        unauthorized_account.id(), // Note sender is NOT the bridge operator
        ger_value_u32s,
        ger_index,
        serial_num,
    )?;

    // Execute UPDATE_GER note against bridge account - should fail
    let tx_context = mock_chain
        .build_tx_context(bridge_account.id(), &[], &[update_ger_note])?
        .build()?;
    let result = tx_context.execute().await;

    // Verify that the transaction failed due to authorization
    assert!(result.is_err(), "transaction should fail when called by unauthorized account");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("note sender is not the authorized bridge operator"),
        "error should mention authorization failure, got: {}",
        error_msg
    );

    Ok(())
}
