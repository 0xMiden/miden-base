use assert_matches::assert_matches;
use miden_lib::{
    account::wallets::{BasicWallet, create_basic_wallet},
    transaction::{TransactionKernel, TransactionKernelError},
};
use miden_objects::{
    Felt, FieldElement,
    account::{
        AccountBuilder, AccountComponent, AccountId, AccountStorage, AccountStorageMode,
        AccountType,
    },
    testing::{
        account_component::AccountMockComponent, account_id::ACCOUNT_ID_SENDER, note::NoteBuilder,
    },
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain};
use miden_tx::TransactionExecutorError;
use vm_processor::ExecutionError;

#[test]
fn test_multisig() -> anyhow::Result<()> {
    let assembler = TransactionKernel::assembler();

    let account_1 = create_basic_wallet(
        [0; 32],
        Auth::BasicAuth,
        AccountType::RegularAccountImmutableCode,
        &mut rand::rng(),
    )?;
    let account_2 = create_basic_wallet(
        [0; 32],
        Auth::BasicAuth,
        AccountType::RegularAccountImmutableCode,
        &mut rand::rng(),
    )?;

    let (multisig_auth_component, authenticator) = Auth::Multisig {
        threshold: 2,
        signers: vec![account_1.id(), account_2.id()],
    }
    .build_component();

    let multisig_account = AccountBuilder::new([0; 32])
        .with_auth_component(multisig_auth_component)
        // TODO is multisig also just a BasicWallet in terms of capabilities?
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain = MockChain::new();
    mock_chain.add_pending_account(multisig_account.clone());
    mock_chain.add_pending_account(account_1.clone());
    mock_chain.add_pending_account(account_2.clone());

    // Create a mock note to consume (needed to make the transaction non-empty)
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;

    let note = NoteBuilder::new(sender_id, &mut rand::rng())
        .build(&assembler)
        .expect("failed to create mock note");

    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block()?;

    // INIT TX AND ADD FIRST SIGNATURE
    // -------------
    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_id);
    let asset = Word::from(FungibleAsset::new(faucet_id, 10)?);

    // This script is executed in a special kernel that doesn't do auth (before epilogue)
    let tx_script_init_tx = format!(
        "
        use.miden::tx

        begin
            push.{recipient}
            push.{NOTE_EXECUTION_HINT}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx]

            push.{asset}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw
            # => [note_idx]

            # truncate the stack
            swapdw dropw dropw
        end
        ",
        recipient = word_to_masm_push_string(&recipient),
        PUBLIC_NOTE = NoteType::Public as u8,
        NOTE_EXECUTION_HINT = Felt::from(NoteExecutionHint::always()),
        tag = tag,
        asset = word_to_masm_push_string(&asset),
    );

    let tx_script_send_note = TransactionScript::compile(
        tx_script_init_tx,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    let tx_context_init_tx = mock_chain
        .build_tx_context(multisig_auth_component.id(), &[], &[note.clone()])?
        .authenticator(authenticator.clone())
        .tx_script(tx_script_send_note.clone())
        .build()?;

    // Calling `execute_special` kernel that advances through the main tx script except for the epilogue,
    // Or otherwise getting the commitments and breaking out from execution.
    let executed_tx_init_tx = tx_context_init_tx.execute_special()?;

    let account_delta = executed_tx_init_tx.account_delta();
    let input_notes_commitment = executed_tx_init_tx.input_notes().commitment();
    let output_notes_commitment = executed_tx_init_tx.output_notes().commitment();

    let message = Hasher::hash(&[
        account_delta.commitment(),
        input_notes_commitment,
        output_notes_commitment,
    ]);

    // No tx script, but we pass the `message` as auth arguments
    let tx_context_add_sig = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[note.clone()])?
        .authenticator(account_1_authenticator.clone())
        .auth_arguments(message)
        .build()?;

    tx_context_add_sig.execute().expect("adding signature should succeed");

    // // Using MockChain we have two separate steps (`execute_special`, then `execute`), but we can simplify it for the rust client.
    //
    // // TODO: do we make a new MultisigClient (and maybe MultisigTransactionRequestBuilder?),
    // // or modify the existing `Client`?
    //
    // // This is how it would look to the client:
    // // The app developer calls a high-level method `multisig_transaction`.
    // let client = MultisigClient::new(..);
    // let transaction_request = TransactionRequestBuilder::new()
    //     .multisig_transaction(_notes, tx_script_send_note);
    //
    // // Under the hood, the client:
    // // - handles tx simulation (or otherwise gets commitments)
    // // - figures out if this is the first, middle, or last tx
    // // - if first (or middle), the client only simulates note and tx script execution,
    // //   "strips away" the actual notes and passed tx script, and only passes the hashed message
    // //   to `add_signature`. This way, the commitments are empty in the auth procedure.
    // // - if last, the client uses the real inputs etc. Then `all_commitments_empty = false`.
    // // Depending on whether we want an explicitly separate `approve` & `execute` or under one method (in `multisig.masm` they are currently assumed to be together), also add own signature before executing the multisig state transition.

    // let tx_result = client
    //     .new_transaction(account.id(), transaction_request);

    // ADD SECOND SIGNATURE AND EXECUTE
    // -------------
    // Here we would need a way to pass the transaction around
    // let (_notes, tx_script_send_note) = private_data.deserialize();

    // The actual execute script is the same, just executed in a normal context
    let tx_context_add_sig_2 = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[note.clone()])?
        .authenticator(account_2_authenticator.clone())
        .tx_script(tx_script_send_note)
        // We pass the message as an auth argument, this way we can just use the original tx script (instead of modifying the state-transition tx script to explicitly call `add_sig`)
        .auth_arguments(message)
        .build()?;

    tx_context_add_sig_2.execute().expect("adding signature should succeed");

    Ok(())
}
