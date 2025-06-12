extern crate alloc;

use miden_crypto::Word;
use miden_lib::{
    errors::tx_kernel_errors::ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED,
    note::utils::build_p2id_recipient,
    transaction::{TransactionKernel, TransactionKernelError},
};
use miden_objects::{
    Felt,
    asset::{Asset, FungibleAsset},
    note::{NoteAssets, NoteExecutionHint, NoteId, NoteMetadata, NoteTag, NoteType},
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain, MockFungibleFaucet};
use miden_tx::{TransactionExecutorError, utils::word_to_masm_push_string};
use vm_processor::ExecutionError;

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
    let faucet = mock_chain.add_pending_existing_faucet(Auth::BasicAuth, "TST", 200, None);

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
        TransactionScript::compile(tx_script_code, vec![], TransactionKernel::testing_assembler())
            .unwrap();
    let tx_context = mock_chain
        .build_tx_context(faucet.account().id(), &[], &[])
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
    let faucet: MockFungibleFaucet =
        mock_chain.add_pending_existing_faucet(Auth::BasicAuth, "TST", 200u64, None);

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
        TransactionScript::compile(tx_script_code, vec![], TransactionKernel::testing_assembler())
            .unwrap();
    let tx = mock_chain
        .build_tx_context(faucet.account().id(), &[], &[])
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
    let faucet = mock_chain.add_pending_existing_faucet(Auth::BasicAuth, "TST", 200, Some(100));

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
    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------
    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(faucet.account().id(), &[note.id()], &[])
        .build()
        .execute()
        .unwrap();

    // Prove, serialize/deserialize and verify the transaction
    prove_and_verify_transaction(executed_transaction.clone()).unwrap();

    // check that the account burned the asset
    assert_eq!(executed_transaction.account_delta().nonce(), Some(Felt::new(3)));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), note.id());
}

#[test]
fn burn_and_mint_fungible_asset() {
    let mut mock_chain = MockChain::new();
    let faucet = mock_chain.add_pending_existing_faucet(Auth::BasicAuth, "TST", u64::MAX, Some(1));
    let faucet_other =
        mock_chain.add_pending_existing_faucet(Auth::BasicAuth, "TST", u64::MAX, Some(1));

    let mut faucet_account = faucet.account().clone();
    let receiver = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, Vec::new());

    let fungible_asset = FungibleAsset::new(faucet.account().id(), 1).unwrap();

    let recipient = build_p2id_recipient(receiver.id(), Word::default()).unwrap();

    let aux = Felt::new(27);
    let tag = NoteTag::for_local_use_case(123, 0).unwrap();
    let amount = Felt::new(250);
    let note_execution_hint = NoteExecutionHint::Always;
    let note_type = NoteType::Private;

    tag.validate(note_type).expect("note tag should support private notes");

    let note_script = format!(
        "
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
            dropw dropw dropw dropw

            push.{recipient}
            push.{note_execution_hint}
            push.{note_type}
            push.{aux}
            push.{tag}
            push.{amount}
            # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

            call.::miden::contracts::faucets::basic_fungible::distribute
            # => [note_idx, pad(15)]

            # truncate the stack
            dropw dropw dropw dropw
        end",
        note_type = note_type as u8,
        recipient = word_to_masm_push_string(&recipient.digest()),
        note_execution_hint = Felt::from(note_execution_hint)
    );

    let note = get_note_with_fungible_asset_and_script(fungible_asset, note_script.as_str());

    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block();

    // Execute the transaction against the receiver(BasicWallet).
    // Should fail due to account authentication in `authenticate_account_origin`:
    // `burn` procedure is not in the set of the receipient's account procedures.
    let executed_transaction_receiver =
        mock_chain.build_tx_context(receiver.id(), &[note.id()], &[]).build().execute();

    match executed_transaction_receiver.unwrap_err() {
        TransactionExecutorError::TransactionProgramExecutionFailed(
            ExecutionError::EventError { label: _, source_file: _, error },
        ) => {
            if let Some(kernel_error) = error.downcast_ref::<TransactionKernelError>() {
                match kernel_error {
                    TransactionKernelError::UnknownAccountProcedure(_digest) => {},
                    e => panic!("Expected UnknownAccountProcedure, got: {:?}", e),
                }
            } else {
                panic!("Failed to downcast EventError to TransactionKernelError, got: {:?}", error);
            }
        },
        e => panic!("Expected TransactionProgramExecutionFailed error, got: {:?}", e),
    }

    // Execute the transaction against the other faucet.
    // Should fail due to the mismatch between the caller and the origin of the asset as part of
    // `validate_fungible_asset_origin`:
    // `burn` and `mint` are only callable by the original faucet
    let executed_transaction_other = mock_chain
        .build_tx_context(faucet_other.account().id(), &[note.id()], &[])
        .build()
        .execute();

    match executed_transaction_other.unwrap_err() {
        TransactionExecutorError::TransactionProgramExecutionFailed(
            ExecutionError::FailedAssertion {
                label: _,
                source_file: _,
                clk: _,
                err_code: _,
                err_msg,
            },
        ) => {
            if let Some(ref msg) = err_msg {
                assert!(msg.contains("the origin of the fungible asset is not this faucet"));
            }
        },
        e => panic!("Expected TransactionProgramExecutionFailed error, got: {:?}", e),
    }

    // Finally, execute the transaction against the original faucet. Should succeed
    let executed_transaction = mock_chain
        .build_tx_context(faucet.account().id(), &[note.id()], &[])
        .build()
        .execute();

    assert!(executed_transaction.is_ok());
    let executed_transaction = executed_transaction.unwrap();

    faucet_account.apply_delta(executed_transaction.account_delta()).unwrap();
    assert_eq!(faucet_account.storage().get_item(0).unwrap()[3], amount);
}
