extern crate alloc;

use core::slice;

use miden_lib::account::wallets::BasicWallet;
use miden_lib::agglayer::{
    create_agglayer_faucet_builder,
    create_bridge_account_builder,
    create_claim_note,
};
use miden_lib::note::WellKnownNote;
use miden_objects::account::Account;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::crypto::rand::FeltRng;
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

/// Tests the bridge-in flow: CLAIM note -> Aggfaucet (FPI to Bridge) -> P2ID note created.
#[tokio::test]
async fn test_bridge_in_claim_to_p2id() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // CREATE BRIDGE ACCOUNT (with bridge_out component for MMR validation)
    // --------------------------------------------------------------------------------------------
    let bridge_seed = builder.rng_mut().draw_word();
    let bridge_account_builder = create_bridge_account_builder(bridge_seed);
    let bridge_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        bridge_account_builder,
        AccountState::Exists,
    )?;

    // CREATE AGGLAYER FAUCET ACCOUNT (with agglayer_faucet component)
    // --------------------------------------------------------------------------------------------
    let token_symbol = "AGG";
    let decimals = 8u8;
    let max_supply = Felt::new(1000000);
    let agglayer_faucet_seed = builder.rng_mut().draw_word();

    let agglayer_faucet_builder = create_agglayer_faucet_builder(
        agglayer_faucet_seed,
        token_symbol,
        decimals,
        max_supply,
        bridge_account.id(),
    );
    let agglayer_faucet = builder.add_account_from_builder(
        Auth::IncrNonce,
        agglayer_faucet_builder,
        AccountState::Exists,
    )?;

    // CREATE USER ACCOUNT TO RECEIVE P2ID NOTE
    // --------------------------------------------------------------------------------------------
    let user_account_builder =
        Account::builder(builder.rng_mut().random()).with_component(BasicWallet);
    let user_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        user_account_builder,
        AccountState::Exists,
    )?;

    // CREATE CLAIM NOTE WITH P2ID OUTPUT NOTE DETAILS
    // --------------------------------------------------------------------------------------------
    let amount = Felt::new(100);
    let aux = Felt::new(0);
    let serial_num = Word::from([1, 2, 3, 4u32]);

    // Create P2ID note for the user account (similar to network faucet test)
    let p2id_script = WellKnownNote::P2ID.script();
    let p2id_inputs = vec![user_account.id().suffix(), user_account.id().prefix().as_felt()];
    let note_inputs = NoteInputs::new(p2id_inputs)?;
    let p2id_recipient = NoteRecipient::new(serial_num, p2id_script.clone(), note_inputs);

    // Create CLAIM note using the helper function
    let claim_note = create_claim_note(
        agglayer_faucet.id(),
        agglayer_faucet.id(),
        user_account.id(),
        amount,
        &p2id_script,
        serial_num,
        aux,
        builder.rng_mut(),
    )?;

    // Add the claim note to the builder before building the mock chain
    builder.add_output_note(OutputNote::Full(claim_note.clone()));

    // BUILD MOCK CHAIN WITH ALL ACCOUNTS
    // --------------------------------------------------------------------------------------------
    let mut mock_chain = builder.clone().build()?;
    mock_chain.prove_next_block()?;

    // CREATE EXPECTED P2ID NOTE FOR VERIFICATION
    // --------------------------------------------------------------------------------------------
    let mint_asset: Asset = FungibleAsset::new(agglayer_faucet.id(), amount.into())?.into();
    let output_note_tag = NoteTag::from_account_id(user_account.id());
    let expected_p2id_note = Note::new(
        NoteAssets::new(vec![mint_asset])?,
        NoteMetadata::new(
            agglayer_faucet.id(),
            NoteType::Public,
            output_note_tag,
            NoteExecutionHint::always(),
            aux,
        )?,
        p2id_recipient,
    );

    // EXECUTE CLAIM NOTE AGAINST AGGLAYER FAUCET (with FPI to Bridge)
    // --------------------------------------------------------------------------------------------
    let foreign_account_inputs = mock_chain.get_foreign_account_inputs(bridge_account.id())?;

    let tx_context = mock_chain
        .build_tx_context(agglayer_faucet.id(), &[], &[claim_note])?
        .add_note_script(p2id_script)
        .foreign_accounts(vec![foreign_account_inputs])
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    // VERIFY P2ID NOTE WAS CREATED
    // --------------------------------------------------------------------------------------------

    // Check that a P2ID note was created by the faucet
    assert_eq!(executed_transaction.output_notes().num_notes(), 1);
    let output_note = executed_transaction.output_notes().get_note(0);

    // Verify the output note contains the minted fungible asset
    let expected_asset = FungibleAsset::new(agglayer_faucet.id(), amount.into())?;

    // Verify the note was created by the agglayer faucet
    assert_eq!(output_note.metadata().sender(), agglayer_faucet.id());
    assert_eq!(output_note.metadata().note_type(), NoteType::Public);
    assert_eq!(output_note.id(), expected_p2id_note.id());

    // Extract the full note from the OutputNote enum for detailed verification
    let full_note = match output_note {
        OutputNote::Full(note) => note,
        _ => panic!("Expected OutputNote::Full variant for public note"),
    };

    // Verify the output note contains the expected fungible asset
    let expected_asset_obj = Asset::from(expected_asset);
    assert!(full_note.assets().iter().any(|asset| asset == &expected_asset_obj));

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_transaction)?;
    mock_chain.prove_next_block()?;

    // CONSUME THE OUTPUT NOTE WITH TARGET ACCOUNT
    // --------------------------------------------------------------------------------------------
    // Consume the output note with target account
    let mut user_account_mut = user_account.clone();
    let consume_tx_context = mock_chain
        .build_tx_context(user_account_mut.clone(), &[], slice::from_ref(&expected_p2id_note))?
        .build()?;
    let consume_executed_transaction = consume_tx_context.execute().await?;

    user_account_mut.apply_delta(consume_executed_transaction.account_delta())?;

    // Verify the account's vault now contains the expected fungible asset
    let balance = user_account_mut.vault().get_balance(agglayer_faucet.id())?;
    assert_eq!(balance, expected_asset.amount());

    Ok(())
}
