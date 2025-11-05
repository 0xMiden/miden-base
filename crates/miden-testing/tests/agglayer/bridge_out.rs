extern crate alloc;

use miden_lib::account::wallets::BasicWallet;
use miden_lib::agglayer::{b2agg_script, bridge_out_component};
use miden_objects::account::{Account, AccountStorageMode};
use miden_objects::asset::{Asset, FungibleAsset};
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
use miden_objects::{Felt, Word};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests the B2AGG (Bridge to AggLayer) note script with bridge_out account component.
///
/// This test:
/// 1. Creates a bridge account with the bridge_out component
/// 2. Creates a B2AGG note that will be consumed by the bridge account
/// 3. Executes the note consumption, which calls the bridge_out component
#[tokio::test]
async fn test_bridge_out_consumes_b2agg_note() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create a faucet to provide assets for the B2AGG note
    let faucet = builder.add_existing_basic_faucet(Auth::BasicAuth, "TST", 1000, Some(100))?;

    // Create a bridge account with the bridge_out component
    let bridge_component = bridge_out_component(vec![]);
    let account_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(BasicWallet)
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
    let note_type = NoteType::Private;

    // Get the B2AGG note script
    let b2agg_script = b2agg_script();

    // Create note inputs (empty for now)
    let inputs = NoteInputs::new(vec![])?;

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

    // EXECUTE B2AGG NOTE AGAINST BRIDGE ACCOUNT
    // --------------------------------------------------------------------------------------------
    let tx_context = mock_chain
        .build_tx_context(bridge_account.id(), &[b2agg_note.id()], &[])?
        .build()?;
    let executed_transaction = tx_context.execute().await?;

    // Verify the transaction executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), b2agg_note.id());

    // Apply the delta to the bridge account
    bridge_account.apply_delta(executed_transaction.account_delta())?;

    // Verify the bridge account received the asset
    let balance = bridge_account.vault().get_balance(faucet.id())?;
    assert_eq!(balance, amount.as_int());

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_transaction)?;
    mock_chain.prove_next_block()?;

    Ok(())
}
