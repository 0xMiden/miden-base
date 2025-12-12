extern crate alloc;

use miden_lib::account::wallets::BasicWallet;
use miden_lib::agglayer::{agglayer_faucet_component, bridge_out_component, claim_script};
use miden_lib::note::WellKnownNote;
use miden_objects::account::{
    Account,
    AccountId,
    AccountStorageMode,
    StorageSlot,
    StorageSlotName,
};
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
use miden_objects::{Felt, Word};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests the bridge-in flow: CLAIM note -> Aggfaucet (FPI to Bridge) -> P2ID note created.
#[tokio::test]
async fn test_bridge_in_claim_to_p2id() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // CREATE BRIDGE ACCOUNT (with bridge_out component for MMR validation)
    // --------------------------------------------------------------------------------------------
    let bridge_storage_slot_name = StorageSlotName::new("miden::agglayer::bridge").unwrap();
    let bridge_storage_slots = vec![StorageSlot::with_empty_map(bridge_storage_slot_name)];
    let bridge_component = bridge_out_component(bridge_storage_slots);
    let bridge_account_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(bridge_component);
    let bridge_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        bridge_account_builder,
        AccountState::Exists,
    )?;

    println!(
        "bridge account id: {} {}",
        bridge_account.id().prefix().as_felt(),
        bridge_account.id().suffix()
    );

    let test_parse =
        AccountId::try_from([bridge_account.id().prefix().as_felt(), bridge_account.id().suffix()])
            .unwrap();
    assert_eq!(test_parse, bridge_account.id());

    // CREATE AGGLAYER FAUCET ACCOUNT (with agglayer_faucet component)
    // --------------------------------------------------------------------------------------------
    let bridge_account_id_word = Word::new([
        Felt::new(0),
        Felt::new(0),
        bridge_account.id().suffix(),
        bridge_account.id().prefix().as_felt(),
    ]);
    let agglayer_storage_slot_name = StorageSlotName::new("miden::agglayer::faucet").unwrap();
    let agglayer_storage_slots =
        vec![StorageSlot::with_value(agglayer_storage_slot_name, bridge_account_id_word)];
    let agglayer_component = agglayer_faucet_component(agglayer_storage_slots);
    let agglayer_faucet_builder = Account::builder(builder.rng_mut().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(agglayer_component);
    let agglayer_faucet = builder.add_account_from_builder(
        Auth::IncrNonce,
        agglayer_faucet_builder,
        AccountState::Exists,
    )?;

    // CREATE USER ACCOUNT TO RECEIVE P2ID NOTE
    // --------------------------------------------------------------------------------------------
    let user_account_builder =
        Account::builder(builder.rng_mut().random()).with_component(BasicWallet);
    let _user_account = builder.add_account_from_builder(
        Auth::IncrNonce,
        user_account_builder,
        AccountState::Exists,
    )?;

    // BUILD MOCK CHAIN WITH ALL ACCOUNTS
    // --------------------------------------------------------------------------------------------
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // CREATE CLAIM NOTE WITH BRIDGE METADATA
    // --------------------------------------------------------------------------------------------
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let aux = Felt::new(0);
    let note_execution_hint = NoteExecutionHint::always();
    let note_type = NoteType::Public;

    let claim_script = claim_script();

    let inputs = NoteInputs::new(vec![])?;
    let claim_note_metadata =
        NoteMetadata::new(agglayer_faucet.id(), note_type, tag, note_execution_hint, aux)?;
    let claim_note_assets = NoteAssets::new(vec![])?; // Empty assets - will be validated and minted
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let claim_note_recipient = NoteRecipient::new(serial_num, claim_script, inputs);
    let claim_note = Note::new(claim_note_assets, claim_note_metadata, claim_note_recipient);

    // EXECUTE CLAIM NOTE AGAINST AGGLAYER FAUCET (with FPI to Bridge)
    // --------------------------------------------------------------------------------------------

    let p2id_note_script: NoteScript = WellKnownNote::P2ID.script();
    let foreign_account_inputs = mock_chain.get_foreign_account_inputs(bridge_account.id())?;

    let tx_context = mock_chain
        .build_tx_context(agglayer_faucet.id(), &[], &[claim_note])?
        .add_note_script(p2id_note_script.clone())
        .foreign_accounts(vec![foreign_account_inputs])
        .build()?;

    let _executed_transaction = tx_context.execute().await?;

    // VERIFY P2ID NOTE WAS CREATED
    // --------------------------------------------------------------------------------------------

    Ok(())
}
