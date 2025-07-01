extern crate alloc;

use miden_lib::{
    errors::tx_kernel_errors::ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED,
    transaction::TransactionKernel,
};
use miden_objects::{
    Felt,
    asset::{Asset, FungibleAsset},
    note::{NoteAssets, NoteExecutionHint, NoteId, NoteMetadata, NoteTag, NoteType},
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain, MockFungibleFaucet};
use miden_tx::utils::word_to_masm_push_string;

use crate::{
    assert_transaction_executor_error, get_note_with_fungible_asset_and_script,
    prove_and_verify_transaction,
};

// TESTS MINT FUNGIBLE ASSET
// ================================================================================================

#[test]
fn prove_faucet_contract_mint_fungible_asset_succeeds() {
    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------
    let mut mock_chain = MockChain::new();
    let faucet = mock_chain
        .add_pending_existing_faucet(Auth::BasicAuth, "TST", 200, None)
        .expect("failed to add pending existing faucet");

    let recipient = [Felt::new(0), Felt::new(1), Felt::new(2), Felt::new(3)];
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let aux = Felt::new(27);
    let note_execution_hint = NoteExecutionHint::on_block_slot(5, 6, 7);
    let note_type = NoteType::Private;
    let amount = Felt::new(100);

    tag.validate(note_type).expect("note tag should support private notes");

    let tx_script_code = format!(
        "
            begin
                # pad the stack before call
                push.0.0.0 padw

                push.{recipient}
                push.{note_execution_hint}
                push.{note_type}
                push.{aux}
                push.{tag}
                push.{amount}
                # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

                call.::miden::contracts::faucets::basic_fungible::distribute
                # => [note_idx, pad(15)]

                call.::miden::contracts::auth::basic::auth_tx_rpo_falcon512
                # => [note_idx, pad(15)]

                # truncate the stack
                dropw dropw dropw dropw
            end
            ",
        note_type = note_type as u8,
        recipient = word_to_masm_push_string(&recipient),
        aux = aux,
        tag = u32::from(tag),
        note_execution_hint = Felt::from(note_execution_hint)
    );

    let tx_script =
        TransactionScript::compile(tx_script_code, TransactionKernel::testing_assembler()).unwrap();
    let tx_context = mock_chain
        .build_tx_context(faucet.account().id(), &[], &[])
        .unwrap()
        .tx_script(tx_script)
        .build();

    let executed_transaction = tx_context.execute().unwrap();

    prove_and_verify_transaction(executed_transaction.clone()).unwrap();

    let fungible_asset: Asset =
        FungibleAsset::new(faucet.account().id(), amount.into()).unwrap().into();

    let output_note = executed_transaction.output_notes().get_note(0).clone();

    let assets = NoteAssets::new(vec![fungible_asset]).unwrap();
    let id = NoteId::new(recipient.into(), assets.commitment());

    assert_eq!(output_note.id(), id);
    assert_eq!(
        output_note.metadata(),
        &NoteMetadata::new(faucet.account().id(), NoteType::Private, tag, note_execution_hint, aux)
            .unwrap()
    );
}

#[test]
fn faucet_contract_mint_fungible_asset_fails_exceeds_max_supply() {
    // CONSTRUCT AND EXECUTE TX (Failure)
    // --------------------------------------------------------------------------------------------
    let mut mock_chain = MockChain::new();
    let faucet: MockFungibleFaucet = mock_chain
        .add_pending_existing_faucet(Auth::BasicAuth, "TST", 200u64, None)
        .expect("failed to add pending existing faucet");

    let recipient = [Felt::new(0), Felt::new(1), Felt::new(2), Felt::new(3)];
    let aux = Felt::new(27);
    let tag = Felt::new(4);
    let amount = Felt::new(250);

    let tx_script_code = format!(
        "
            begin
                # pad the stack before call
                push.0.0.0 padw

                push.{recipient}
                push.{note_type}
                push.{aux}
                push.{tag}
                push.{amount}
                # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

                call.::miden::contracts::faucets::basic_fungible::distribute
                # => [note_idx, pad(15)]

                call.::miden::contracts::auth::basic::auth_tx_rpo_falcon512
                # => [note_idx, pad(15)]

                # truncate the stack
                dropw dropw dropw dropw

            end
            ",
        note_type = NoteType::Private as u8,
        recipient = word_to_masm_push_string(&recipient),
    );

    let tx_script =
        TransactionScript::compile(tx_script_code, TransactionKernel::testing_assembler()).unwrap();
    let tx = mock_chain
        .build_tx_context(faucet.account().id(), &[], &[])
        .unwrap()
        .tx_script(tx_script)
        .build()
        .execute();

    // Execute the transaction and get the witness
    assert_transaction_executor_error!(
        tx,
        ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED
    );
}

// TESTS BURN FUNGIBLE ASSET
// ================================================================================================

#[test]
fn prove_faucet_contract_burn_fungible_asset_succeeds() {
    let mut mock_chain = MockChain::new();
    let faucet = mock_chain
        .add_pending_existing_faucet(Auth::BasicAuth, "TST", 200, Some(100))
        .expect("failed to add pending existing faucet");

    let fungible_asset = FungibleAsset::new(faucet.account().id(), 100).unwrap();

    // The Fungible Faucet component is added as the first component, so it's storage slot offset
    // will be 1. Check that max_supply at the word's index 0 is 200. The remainder of the word
    // is initialized with the metadata of the faucet which we don't need to check.
    assert_eq!(faucet.account().storage().get_item(1).unwrap()[0], Felt::new(200));

    // Check that the faucet reserved slot has been correctly initialized.
    // The already issued amount should be 100.
    assert_eq!(faucet.account().storage().get_item(0).unwrap()[3], Felt::new(100));

    // need to create a note with the fungible asset to be burned
    let note_script = "
        # burn the asset
        begin
            dropw

            # pad the stack before call
            padw padw padw padw
            # => [pad(16)]

            exec.::miden::note::get_assets drop
            mem_loadw
            # => [ASSET, pad(12)]

            call.::miden::contracts::faucets::basic_fungible::burn

            # truncate the stack
            dropw dropw dropw dropw
        end
        ";

    let note = get_note_with_fungible_asset_and_script(fungible_asset, note_script);

    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block().unwrap();

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------
    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(faucet.account().id(), &[note.id()], &[])
        .unwrap()
        .build()
        .execute()
        .unwrap();

    // Prove, serialize/deserialize and verify the transaction
    prove_and_verify_transaction(executed_transaction.clone()).unwrap();

    // nonce was incremented by 2 (once by the call to burn, once by the auth script)
    assert_eq!(executed_transaction.account_delta().nonce(), Some(Felt::new(2)));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), note.id());
}
