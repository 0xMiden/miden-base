use miden_lib::{note::create_swap_note, transaction::TransactionKernel};
use miden_objects::{
    Felt,
    account::AccountId,
    asset::{Asset, NonFungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::{Note, NoteDetails, NoteType},
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain};
use miden_tx::utils::word_to_masm_push_string;

use crate::prove_and_verify_transaction;

// Creates a swap note and sends it with send_asset
#[test]
pub fn prove_send_swap_note() {
    let mut mock_chain = MockChain::new();
    let offered_asset =
        mock_chain.add_pending_new_faucet(Auth::BasicAuth, "USDT", 100000u64).mint(2000);
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);
    let sender_account =
        mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![offered_asset]);

    let (note, _payback) = get_swap_notes(sender_account.id(), offered_asset, requested_asset);

    // CREATE SWAP NOTE TX
    // --------------------------------------------------------------------------------------------

    let tx_script_src = &format!(
        "
        begin
            push.{recipient}
            push.{note_execution_hint}
            push.{note_type}
            push.0              # aux
            push.{tag}
            call.::miden::contracts::wallets::basic::create_note

            push.{asset}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            call.::miden::contracts::auth::basic::auth_tx_rpo_falcon512
            dropw dropw dropw dropw
        end
        ",
        recipient = word_to_masm_push_string(&note.recipient().digest()),
        note_type = NoteType::Public as u8,
        tag = Felt::from(note.metadata().tag()),
        asset = word_to_masm_push_string(&offered_asset.into()),
        note_execution_hint = Felt::from(note.metadata().execution_hint())
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler()).unwrap();

    let create_swap_note_tx = mock_chain
        .build_tx_context(sender_account.id(), &[], &[])
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![OutputNote::Full(note.clone())])
        .build()
        .execute()
        .unwrap();

    let sender_account = mock_chain.add_pending_executed_transaction(&create_swap_note_tx);

    assert!(
        create_swap_note_tx
            .output_notes()
            .iter()
            .any(|n| n.commitment() == note.commitment())
    );
    assert_eq!(sender_account.vault().assets().count(), 0); // Offered asset should be gone
    let swap_output_note = create_swap_note_tx.output_notes().iter().next().unwrap();
    assert_eq!(swap_output_note.assets().unwrap().iter().next().unwrap(), &offered_asset);
    assert!(prove_and_verify_transaction(create_swap_note_tx).is_ok());
}

// Consumes the swap note (same as the one used in the above test) and proves the transaction
// The sender account also consumes the payback note
#[test]
fn prove_consume_swap_note() {
    let mut mock_chain = MockChain::new();
    let offered_asset =
        mock_chain.add_pending_new_faucet(Auth::BasicAuth, "USDT", 100000u64).mint(2000);
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);
    let sender_account =
        mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![offered_asset]);

    let (note, payback_note) = get_swap_notes(sender_account.id(), offered_asset, requested_asset);

    // CONSUME CREATED NOTE
    // --------------------------------------------------------------------------------------------

    let target_account =
        mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![requested_asset]);
    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block();

    let consume_swap_note_tx = mock_chain
        .build_tx_context(target_account.id(), &[note.id()], &[])
        .build()
        .execute()
        .unwrap();

    let target_account = mock_chain.add_pending_executed_transaction(&consume_swap_note_tx);

    let output_payback_note = consume_swap_note_tx.output_notes().iter().next().unwrap().clone();
    assert!(output_payback_note.id() == payback_note.id());
    assert_eq!(output_payback_note.assets().unwrap().iter().next().unwrap(), &requested_asset);

    assert!(prove_and_verify_transaction(consume_swap_note_tx).is_ok());
    assert!(target_account.vault().assets().count() == 1);
    assert!(target_account.vault().assets().any(|asset| asset == offered_asset));

    // CONSUME PAYBACK P2ID NOTE
    // --------------------------------------------------------------------------------------------

    let full_payback_note = Note::new(
        payback_note.assets().clone(),
        *output_payback_note.metadata(),
        payback_note.recipient().clone(),
    );

    let consume_payback_tx = mock_chain
        .build_tx_context(sender_account.id(), &[], &[full_payback_note])
        .build()
        .execute()
        .unwrap();

    let sender_account = mock_chain.add_pending_executed_transaction(&consume_payback_tx);
    assert!(sender_account.vault().assets().any(|asset| asset == requested_asset));
    assert!(prove_and_verify_transaction(consume_payback_tx).is_ok());
}

fn get_swap_notes(
    sender_account_id: AccountId,
    offered_asset: Asset,
    requested_asset: Asset,
) -> (Note, NoteDetails) {
    // Create the note containing the SWAP script
    create_swap_note(
        sender_account_id,
        offered_asset,
        requested_asset,
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap()
}
