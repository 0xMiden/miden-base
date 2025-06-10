use miden_crypto::{Word, rand::FeltRng};
use miden_lib::{
    account::wallets::{AuxWallet, BasicWallet},
    note::utils::{build_p2id_recipient, build_swap_tag},
    transaction::TransactionKernel,
};
use miden_objects::{
    Felt,
    account::{Account, AccountId},
    asset::Asset,
    crypto::rand::RpoRandomCoin,
    note::{
        Note, NoteAssets, NoteDetails, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient,
        NoteScript, NoteTag, NoteType,
    },
    transaction::OutputNote,
};
use miden_testing::{AccountState, Auth, MockChain};
use rand::random;

#[test]
fn test_inflight_swap() {
    let mut mock_chain = MockChain::new();
    let offered_asset =
        mock_chain.add_pending_new_faucet(Auth::BasicAuth, "NP", 100_000_u64).mint(200);
    let requested_asset =
        mock_chain.add_pending_new_faucet(Auth::BasicAuth, "MID", 100_000_u64).mint(100);

    // For end users: Create a standard sender account with just BasicWallet
    let mut alice = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![offered_asset]);
    let mut bob = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![requested_asset]);

    let (alice_note, alice_payback_note_details) =
        create_in_flight_swap_note(alice.id(), offered_asset, requested_asset);

    let (bob_note, bob_payback_note_details) =
        create_in_flight_swap_note(bob.id(), requested_asset, offered_asset);

    // For the matcher: Create the account with the AuxWallet as well as BasicWallet components
    let account_builder =
        Account::builder(random()).with_component(BasicWallet).with_component(AuxWallet);

    let mut matcher_account = mock_chain.add_pending_account_from_builder(
        Auth::BasicAuth,
        account_builder,
        AccountState::Exists,
    );

    let consume_swap_note_tx = mock_chain
        .build_tx_context(matcher_account.id(), &[], &[alice_note, bob_note])
        .build()
        .execute()
        .unwrap();

    matcher_account.apply_delta(consume_swap_note_tx.account_delta()).unwrap();
    // The matcher's vault should not have been affected
    assert!(matcher_account.vault().assets().count() == 0);

    // Check that the output notes match alice and bob's payback notes
    let alice_output_payback_note = consume_swap_note_tx
        .output_notes()
        .iter()
        .find(|note| note.id() == alice_payback_note_details.id())
        .unwrap();

    let bob_output_payback_note = consume_swap_note_tx
        .output_notes()
        .iter()
        .find(|note| note.id() == bob_payback_note_details.id())
        .unwrap();

    // Construct the full note that Alice needs to consume, using original details and actual
    // metadata
    let full_alice_payback_note = Note::new(
        alice_payback_note_details.assets().clone(),
        *alice_output_payback_note.metadata(),
        alice_payback_note_details.recipient().clone(),
    );
    let full_bob_payback_note = Note::new(
        bob_payback_note_details.assets().clone(),
        *bob_output_payback_note.metadata(),
        bob_payback_note_details.recipient().clone(),
    );

    mock_chain.add_pending_note(OutputNote::Full(full_alice_payback_note.clone()));
    mock_chain.add_pending_note(OutputNote::Full(full_bob_payback_note.clone()));
    mock_chain.prove_next_block();

    let alice_consume_payback_note_tx = mock_chain
        .build_tx_context(alice.id(), &[full_alice_payback_note.id()], &[])
        .build()
        .execute()
        .unwrap();

    let bob_consume_payback_note_tx = mock_chain
        .build_tx_context(bob.id(), &[full_bob_payback_note.id()], &[])
        .build()
        .execute()
        .unwrap();

    alice.apply_delta(alice_consume_payback_note_tx.account_delta()).unwrap();
    bob.apply_delta(bob_consume_payback_note_tx.account_delta()).unwrap();

    assert!(alice.vault().assets().any(|asset| asset == requested_asset));
    assert!(bob.vault().assets().any(|asset| asset == offered_asset));
}

/// This is a modification of `create_swap_note` to create an in-flight swap note. The consumer of
/// this note does not receive the `offered_asset` directly, and only acts as an intermediary. The
/// consumer will create a new P2ID note with `sender` as target, containing the `requested_asset`.
fn create_in_flight_swap_note(
    sender_account_id: AccountId,
    offered_asset: Asset,
    requested_asset: Asset,
) -> (Note, NoteDetails) {
    let mut rng = RpoRandomCoin::new([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);
    let note_type = NoteType::Public;
    let aux = Felt::new(0);

    let note_script =
        NoteScript::compile(IN_FLIGHT_SWAP_NOTE_SCRIPT, TransactionKernel::testing_assembler())
            .unwrap();

    let payback_serial_num = rng.draw_word();
    let payback_recipient = build_p2id_recipient(sender_account_id, payback_serial_num).unwrap();

    let payback_recipient_word: Word = payback_recipient.digest().into();
    let requested_asset_word: Word = requested_asset.into();
    let payback_tag = NoteTag::from_account_id(sender_account_id);

    let inputs = NoteInputs::new(vec![
        payback_recipient_word[0],
        payback_recipient_word[1],
        payback_recipient_word[2],
        payback_recipient_word[3],
        requested_asset_word[0],
        requested_asset_word[1],
        requested_asset_word[2],
        requested_asset_word[3],
        payback_tag.into(),
        NoteExecutionHint::always().into(),
    ])
    .unwrap();

    let tag = build_swap_tag(note_type, &offered_asset, &requested_asset).unwrap();
    let serial_num = rng.draw_word();

    let metadata =
        NoteMetadata::new(sender_account_id, note_type, tag, NoteExecutionHint::always(), aux)
            .unwrap();
    let assets = NoteAssets::new(vec![offered_asset]).unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, inputs);
    let note = Note::new(assets, metadata, recipient);

    let payback_assets = NoteAssets::new(vec![requested_asset]).unwrap();
    let payback_note = NoteDetails::new(payback_assets, payback_recipient);

    (note, payback_note)
}

const IN_FLIGHT_SWAP_NOTE_SCRIPT: &str = r"
use.miden::note
use.miden::tx
use.miden::contracts::wallets::basic
use.miden::contracts::wallets::aux

# CONSTANTS
# =================================================================================================

const.PRIVATE_NOTE=2

#! Swap script:
#! Creates a note consumable by note issuer containing requested ASSET.
#!
#! Requires that the account exposes:
#! - miden::contracts::wallets::basic::create_note procedure.
#! - miden::contracts::wallets::aux::add_asset_to_note procedure.
#!
#! Inputs:  []
#! Outputs: []
#!
#! Note inputs are assumed to be as follows:
#! - RECIPIENT
#! - ASSET
#! - TAG = [tag, 0, 0, 0]
#!
#! Panics if:
#! - account does not expose miden::contracts::wallets::basic::create_note procedure.
#! - account does not expose miden::contracts::wallets::aux::add_asset_to_note procedure.
begin
    # store note inputs into memory starting at address 0
    push.0 exec.note::get_inputs
    # => [num_inputs, inputs_ptr]

    # make sure the number of inputs is 10
    eq.10 assert
    # => [inputs_ptr]

    # load RECIPIENT
    drop padw mem_loadw
    # => [RECIPIENT]

    padw mem_loadw.4
    # => [ASSET, RECIPIENT]

    padw mem_loadw.8
    # => [0, 0, execution_hint, tag, ASSET, RECIPIENT]

    drop drop swap
    # => [tag, execution_hint, ASSET, RECIPIENT]

    # we add aux = 0 to the note assuming we don't need it for the second leg of the SWAP
    push.0 swap
    # => [tag, aux, execution_hint, ASSET, RECIPIENT]

    push.PRIVATE_NOTE movdn.2
    # => [tag, aux, note_type, execution_hint, ASSET, RECIPIENT]

    swapw
    # => [ASSET, tag, aux, note_type, execution_hint, RECIPIENT]

    # create a note using inputs
    padw swapdw padw movdnw.2
    # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8), ASSET]
    call.basic::create_note
    # => [note_idx, pad(15), ASSET]

    swapw dropw movupw.3
    # => [ASSET, note_idx, pad(11)]


    # move asset to the note
    call.aux::add_asset_to_note
    # => [ASSET, note_idx, pad(11)]

    # clean stack
    dropw dropw dropw dropw
    # => []
end
";
