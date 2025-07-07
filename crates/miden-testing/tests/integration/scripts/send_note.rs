use miden_lib::{account::interface::AccountInterface, transaction::TransactionKernel};
use miden_objects::{
    Felt, ONE,
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
    let mut mock_chain = MockChain::new();
    let sender_basic_wallet_account =
        mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![FungibleAsset::mock(100)]);

    let sender_account_interface = AccountInterface::from(&sender_basic_wallet_account);

    let tag = NoteTag::from_account_id(sender_basic_wallet_account.id());
    let metadata = NoteMetadata::new(
        sender_basic_wallet_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )?;
    let assets = NoteAssets::new(vec![FungibleAsset::mock(10)]).unwrap();
    let note_script =
        NoteScript::compile("begin nop end", TransactionKernel::testing_assembler()).unwrap();
    let serial_num =
        RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]).draw_word();
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
        .build_tx_context(sender_basic_wallet_account.id(), &[], &[])
        .expect("failed to build tx context")
        .tx_script(send_note_transaction_script)
        .extend_expected_output_notes(vec![OutputNote::Full(note)])
        .build()?
        .execute()?;
    Ok(())
}

/// Tests the execution of the generated send_note transaction script in case the sending account
/// has the [`BasicFungibleFaucet`][faucet] interface.
///
/// [faucet]: miden_lib::account::interface::AccountComponentInterface::BasicFungibleFaucet
#[test]
fn test_send_note_script_basic_fungible_faucet() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    let sender_basic_fungible_faucet_account = mock_chain
        .add_pending_existing_faucet(Auth::BasicAuth, "POL", 200, None)
        .expect("failed to add pending existing faucet");

    let sender_account_interface =
        AccountInterface::from(sender_basic_fungible_faucet_account.account());

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
    let serial_num =
        RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]).draw_word();
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
