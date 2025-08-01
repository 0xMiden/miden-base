use std::collections::BTreeMap;

use miden_lib::{account::interface::AccountInterface, transaction::TransactionKernel};
use miden_objects::{
    Word,
    asset::{Asset, FungibleAsset},
    crypto::rand::{FeltRng, RpoRandomCoin},
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient, NoteScript,
        NoteTag, NoteType, PartialNote,
    },
    transaction::OutputNote,
};
use miden_testing::{Auth, MockChain};

/// Tests the execution of the generated send_note transaction script in case the sending account
/// has the [`BasicWallet`][wallet] interface.
///
/// [wallet]: miden_lib::account::interface::AccountComponentInterface::BasicWallet
#[test]
fn test_send_note_script_basic_wallet() -> anyhow::Result<()> {
    let sent_asset = FungibleAsset::mock(10);

    let mut builder = MockChain::builder();
    let sender_basic_wallet_account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, [FungibleAsset::mock(100)])?;
    let mock_chain = builder.build()?;

    let sender_account_interface = AccountInterface::from(&sender_basic_wallet_account);

    let tag = NoteTag::from_account_id(sender_basic_wallet_account.id());
    let metadata = NoteMetadata::new(
        sender_basic_wallet_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )?;
    let assets = NoteAssets::new(vec![sent_asset]).unwrap();
    let note_script =
        NoteScript::compile("begin nop end", TransactionKernel::testing_assembler()).unwrap();
    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());

    let note = Note::new(assets.clone(), metadata, recipient);
    let partial_note: PartialNote = note.clone().into();

    let expiration_delta = 10u16;
    let send_note_transaction_script = sender_account_interface.build_send_notes_script(
        &[partial_note.clone()],
        Some(expiration_delta),
        false,
    )?;

    let executed_transaction = mock_chain
        .build_tx_context(sender_basic_wallet_account.id(), &[], &[])
        .expect("failed to build tx context")
        .tx_script(send_note_transaction_script)
        .extend_expected_output_notes(vec![OutputNote::Full(note)])
        .build()?
        .execute()?;

    // assert that the removed asset is in the delta
    let mut removed_assets: BTreeMap<_, _> = executed_transaction
        .account_delta()
        .vault()
        .removed_assets()
        .map(|asset| (asset.vault_key(), asset))
        .collect();
    assert_eq!(removed_assets.len(), 1, "one asset should have been removed");
    assert_eq!(
        removed_assets.remove(&sent_asset.vault_key()).unwrap(),
        sent_asset,
        "sent asset should be in removed assets"
    );

    Ok(())
}

/// Tests the execution of the generated send_note transaction script in case the sending account
/// has the [`BasicFungibleFaucet`][faucet] interface.
///
/// [faucet]: miden_lib::account::interface::AccountComponentInterface::BasicFungibleFaucet
#[test]
fn test_send_note_script_basic_fungible_faucet() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let sender_basic_fungible_faucet_account =
        builder.add_existing_faucet(Auth::BasicAuth, "POL", 200, None)?;
    let mock_chain = builder.build()?;

    let sender_account_interface = AccountInterface::from(&sender_basic_fungible_faucet_account);

    let tag = NoteTag::from_account_id(sender_basic_fungible_faucet_account.id());
    let metadata = NoteMetadata::new(
        sender_basic_fungible_faucet_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )?;
    let assets = NoteAssets::new(vec![Asset::Fungible(
        FungibleAsset::new(sender_basic_fungible_faucet_account.id(), 10).unwrap(),
    )])?;
    let note_script =
        NoteScript::compile("begin nop end", TransactionKernel::testing_assembler()).unwrap();
    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());

    let note = Note::new(assets.clone(), metadata, recipient);
    let partial_note: PartialNote = note.clone().into();

    let expiration_delta = 10u16;
    let send_note_transaction_script = sender_account_interface.build_send_notes_script(
        &[partial_note.clone()],
        Some(expiration_delta),
        false,
    )?;

    let _executed_transaction = mock_chain
        .build_tx_context(sender_basic_fungible_faucet_account.id(), &[], &[])
        .expect("failed to build tx context")
        .tx_script(send_note_transaction_script)
        .extend_expected_output_notes(vec![OutputNote::Full(note)])
        .build()?
        .execute()?;
    Ok(())
}
