extern crate alloc;

use miden_lib::account::faucets::FungibleFaucetExt;
use miden_lib::agglayer::utils::ethereum_address_string_to_felts;
use miden_lib::agglayer::{b2agg_script, bridge_out_component};
use miden_lib::note::WellKnownNote;
use miden_objects::account::{
    Account,
    AccountId,
    AccountIdVersion,
    AccountStorageMode,
    AccountType,
};
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    NoteTag,
    NoteType,
};
use miden_objects::transaction::OutputNote;
use miden_objects::{Felt, Word};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests the B2AGG (Bridge to AggLayer) note script with bridge_out account component.
///
/// This test flow:
/// 1. Creates a network faucet to provide assets
/// 2. Creates a bridge account with the bridge_out component (using network storage)
/// 3. Creates a B2AGG note with assets from the network faucet
/// 4. Executes the B2AGG note consumption via network transaction
/// 5. Consumes the BURN note
#[tokio::test]
async fn test_bridge_out_consumes_b2agg_note() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create a network faucet owner account
    let faucet_owner_account_id = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    // Create a network faucet to provide assets for the B2AGG note
    let faucet =
        builder.add_existing_network_faucet("AGG", 1000, faucet_owner_account_id, Some(100))?;

    // Create a bridge account with the bridge_out component using network (public) storage
    let bridge_component = bridge_out_component(vec![]);
    let account_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        // .with_component(BasicWallet)
        .with_component(bridge_component);
    let mut bridge_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    // CREATE B2AGG NOTE WITH ASSETS
    // --------------------------------------------------------------------------------------------

    let amount = Felt::new(100);
    let bridge_asset: Asset = FungibleAsset::new(faucet.id(), amount.into()).unwrap().into();
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let aux = Felt::new(0);
    let note_execution_hint = NoteExecutionHint::always();
    let note_type = NoteType::Public; // Use Public note type for network transaction

    // Get the B2AGG note script
    let b2agg_script = b2agg_script();

    // Create note inputs with destination network and address
    // destination_network: u32 (AggLayer-assigned network ID)
    // destination_address: 20 bytes (Ethereum address) split into 5 u32 values
    let destination_network = Felt::new(1); // Example network ID
    let destination_address = "0x1234567890abcdef1122334455667788990011aa";
    let address_felts =
        ethereum_address_string_to_felts(destination_address).expect("Valid Ethereum address");

    // Combine network ID and address felts into note inputs (6 felts total)
    let mut input_felts = vec![destination_network];
    input_felts.extend(address_felts);

    let inputs = NoteInputs::new(input_felts.clone())?;

    // Create the B2AGG note with assets from the faucet
    let b2agg_note_metadata =
        NoteMetadata::new(faucet.id(), note_type, tag, note_execution_hint, aux)?;
    let b2agg_note_assets = NoteAssets::new(vec![bridge_asset])?;
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let b2agg_note_recipient = NoteRecipient::new(serial_num, b2agg_script, inputs);
    let b2agg_note = Note::new(b2agg_note_assets, b2agg_note_metadata, b2agg_note_recipient);

    // Add the B2AGG note to the mock chain
    builder.add_output_note(OutputNote::Full(b2agg_note.clone()));
    let mut mock_chain = builder.build()?;

    // Add BURN note script to the data store so it can be fetched during execution
    let burn_note_script: NoteScript = WellKnownNote::BURN.script();

    // EXECUTE B2AGG NOTE AGAINST BRIDGE ACCOUNT (NETWORK TRANSACTION)
    // --------------------------------------------------------------------------------------------
    let tx_context = mock_chain
        .build_tx_context(bridge_account.id(), &[b2agg_note.id()], &[])?
        .add_note_script(burn_note_script.clone())
        .build()?;
    let executed_transaction = tx_context.execute().await?;

    // Verify the transaction executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), b2agg_note.id());

    // VERIFY PUBLIC BURN NOTE WAS CREATED
    // --------------------------------------------------------------------------------------------
    // The bridge_out component should create a PUBLIC BURN note addressed to the faucet
    assert_eq!(
        executed_transaction.output_notes().num_notes(),
        1,
        "Expected one BURN note to be created"
    );

    let output_note = executed_transaction.output_notes().get_note(0);

    // Extract the full note from the OutputNote enum
    let burn_note = match output_note {
        OutputNote::Full(note) => note,
        _ => panic!("Expected OutputNote::Full variant for BURN note"),
    };

    // Verify the BURN note is public
    assert_eq!(burn_note.metadata().note_type(), NoteType::Public, "BURN note should be public");

    // Verify the BURN note contains the bridged asset
    let expected_asset = FungibleAsset::new(faucet.id(), amount.into())?;
    let expected_asset_obj = Asset::from(expected_asset);
    assert!(
        burn_note.assets().iter().any(|asset| asset == &expected_asset_obj),
        "BURN note should contain the bridged asset"
    );

    // Verify the BURN note is addressed to the faucet
    assert_eq!(
        burn_note.metadata().sender(),
        bridge_account.id(),
        "BURN note should be sent by the bridge account"
    );

    // Verify the BURN note uses the correct script
    assert_eq!(
        burn_note.recipient().script().root(),
        burn_note_script.root(),
        "BURN note should use the BURN script"
    );

    // Apply the delta to the bridge account
    bridge_account.apply_delta(executed_transaction.account_delta())?;

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_transaction)?;
    mock_chain.prove_next_block()?;

    // CONSUME THE BURN NOTE WITH THE NETWORK FAUCET
    // --------------------------------------------------------------------------------------------
    // Check the initial token issuance before burning
    let initial_issuance = faucet.get_token_issuance().unwrap();
    assert_eq!(initial_issuance, Felt::new(100), "Initial issuance should be 100");

    // Execute the BURN note against the network faucet
    let burn_tx_context =
        mock_chain.build_tx_context(faucet.id(), &[burn_note.id()], &[])?.build()?;
    let burn_executed_transaction = burn_tx_context.execute().await?;

    // Verify the burn transaction was successful - no output notes should be created
    assert_eq!(
        burn_executed_transaction.output_notes().num_notes(),
        0,
        "Burn transaction should not create output notes"
    );

    // Verify the transaction was executed successfully
    assert_eq!(
        burn_executed_transaction.account_delta().nonce_delta(),
        Felt::new(1),
        "Faucet nonce should be incremented"
    );
    assert_eq!(
        burn_executed_transaction.input_notes().get_note(0).id(),
        burn_note.id(),
        "Input note should be the BURN note"
    );

    // Apply the delta to the faucet account and verify the token issuance decreased
    let mut faucet = faucet;
    faucet.apply_delta(burn_executed_transaction.account_delta())?;
    let final_issuance = faucet.get_token_issuance().unwrap();
    assert_eq!(
        final_issuance,
        Felt::new(initial_issuance.as_int() - amount.as_int()),
        "Token issuance should decrease by the burned amount"
    );

    Ok(())
}
