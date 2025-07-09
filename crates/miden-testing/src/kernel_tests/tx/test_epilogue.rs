use alloc::{string::ToString, vec::Vec};

use miden_lib::{
    errors::tx_kernel_errors::{
        ERR_ACCOUNT_NONCE_DID_NOT_INCREASE_AFTER_STATE_CHANGE,
        ERR_EPILOGUE_EXECUTED_TRANSACTION_IS_EMPTY,
        ERR_EPILOGUE_TOTAL_NUMBER_OF_ASSETS_MUST_STAY_THE_SAME, ERR_TX_INVALID_EXPIRATION_DELTA,
    },
    transaction::{
        TransactionKernel,
        memory::{NOTE_MEM_SIZE, OUTPUT_NOTE_ASSET_COMMITMENT_OFFSET, OUTPUT_NOTE_SECTION_OFFSET},
    },
};
use miden_objects::{
    FieldElement,
    account::{Account, AccountComponent, AccountDelta, AccountStorageDelta, AccountVaultDelta},
    asset::{Asset, AssetVault, FungibleAsset},
    note::{NoteTag, NoteType},
    testing::{
        account_component::IncrNonceAuthComponent,
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3, ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER,
        },
        constants::{CONSUMED_ASSET_1_AMOUNT, CONSUMED_ASSET_2_AMOUNT, CONSUMED_ASSET_3_AMOUNT},
        note::NoteBuilder,
    },
    transaction::{OutputNote, OutputNotes},
};
use miden_tx::TransactionExecutorError;
use rand::rng;
use vm_processor::{Felt, ONE, ProcessState};

use super::{ZERO, create_mock_notes_procedure};
use crate::{
    Auth, MockChain, TransactionContextBuilder, TxContextInput, assert_execution_error,
    kernel_tests::tx::read_root_mem_word,
    utils::{create_p2any_note, create_spawn_note},
};

#[test]
fn test_epilogue() -> anyhow::Result<()> {
    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );
        let output_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);

        // input_note_1 is needed for maintaining cohesion of involved assets
        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);
        let input_note_2 = create_spawn_note(ACCOUNT_ID_SENDER.try_into()?, vec![&output_note_1])?;
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2])
            .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
            .build()?
    };

    let output_notes_data_procedure =
        create_mock_notes_procedure(tx_context.expected_output_notes());

    let code = format!(
        "
        use.kernel::prologue
        use.kernel::account
        use.kernel::epilogue

        {output_notes_data_procedure}

        begin
            exec.prologue::prepare_transaction

            exec.create_mock_notes

            exec.epilogue::finalize_transaction

            # truncate the stack
            movupw.3 dropw movupw.3 dropw movup.9 drop
        end
        "
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    let assembler = TransactionKernel::assembler();
    let auth_component: AccountComponent =
        IncrNonceAuthComponent::new(assembler.clone()).unwrap().into();
    let final_account = Account::mock(
        tx_context.account().id().into(),
        tx_context.account().nonce() + ONE,
        auth_component,
        assembler,
    );

    let output_notes = OutputNotes::new(
        tx_context
            .expected_output_notes()
            .iter()
            .cloned()
            .map(OutputNote::Full)
            .collect(),
    )?;

    let account_delta_commitment = AccountDelta::new(
        tx_context.account().id(),
        AccountStorageDelta::default(),
        AccountVaultDelta::default(),
        ONE,
    )?
    .commitment();

    let account_update_commitment =
        miden_objects::Hasher::merge(&[final_account.commitment(), account_delta_commitment]);

    let mut expected_stack = Vec::with_capacity(17);
    expected_stack.extend(output_notes.commitment().as_elements().iter().rev());
    expected_stack.extend(account_update_commitment.as_elements().iter().rev());
    expected_stack.push(Felt::from(u32::MAX)); // Value for tx expiration block number
    expected_stack.extend((9..16).map(|_| ZERO));

    assert_eq!(
        *process.stack.build_stack_outputs()?,
        expected_stack.as_slice(),
        "Stack state after finalize_transaction does not contain the expected values"
    );

    assert_eq!(
        process.stack.depth(),
        16,
        "The stack must be truncated to 16 elements after finalize_transaction"
    );
    Ok(())
}

#[test]
fn test_compute_output_note_id() -> anyhow::Result<()> {
    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );
        let output_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into()?, &[FungibleAsset::mock(100)]);

        // input_note_1 is needed for maintaining cohesion of involved assets
        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into()?, &[FungibleAsset::mock(100)]);
        let input_note_2 = create_spawn_note(ACCOUNT_ID_SENDER.try_into()?, vec![&output_note_1])?;
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2])
            .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
            .build()?
    };

    let output_notes_data_procedure =
        create_mock_notes_procedure(tx_context.expected_output_notes());

    for (note, i) in tx_context.expected_output_notes().iter().zip(0u32..) {
        let code = format!(
            "
            use.kernel::prologue
            use.kernel::epilogue

            {output_notes_data_procedure}

            begin
                exec.prologue::prepare_transaction
                exec.create_mock_notes
                exec.epilogue::finalize_transaction

                # truncate the stack
                movupw.3 dropw movupw.3 dropw movup.9 drop
            end
            "
        );

        let process = &tx_context.execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )?;

        assert_eq!(
            note.assets().commitment().as_elements(),
            read_root_mem_word(
                &process.into(),
                OUTPUT_NOTE_SECTION_OFFSET
                    + i * NOTE_MEM_SIZE
                    + OUTPUT_NOTE_ASSET_COMMITMENT_OFFSET
            ),
            "ASSET_COMMITMENT didn't match expected value",
        );

        assert_eq!(
            note.id().as_elements(),
            &read_root_mem_word(&process.into(), OUTPUT_NOTE_SECTION_OFFSET + i * NOTE_MEM_SIZE),
            "NOTE_ID didn't match expected value",
        );
    }
    Ok(())
}

#[test]
fn test_epilogue_asset_preservation_violation_too_few_input() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder
        .add_existing_mock_account_with_assets(Auth::IncrNonce, AssetVault::mock().assets())?;
    let mock_chain = builder.build()?;

    let fungible_asset_1: Asset = FungibleAsset::new(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1.try_into()?,
        CONSUMED_ASSET_1_AMOUNT,
    )?
    .into();
    let fungible_asset_2: Asset = FungibleAsset::new(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into()?,
        CONSUMED_ASSET_2_AMOUNT,
    )?
    .into();

    let output_note_1 = NoteBuilder::new(account.id(), rng())
        .add_assets([fungible_asset_1])
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;
    let output_note_2 = NoteBuilder::new(account.id(), rng())
        .add_assets([fungible_asset_2])
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;

    let input_note = create_spawn_note(account.id(), vec![&output_note_1, &output_note_2])?;

    let tx_context = mock_chain
        .build_tx_context(TxContextInput::AccountId(account.id()), &[], &[input_note])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(output_note_1),
            OutputNote::Full(output_note_2),
        ])
        .build()?;

    let output_notes_data_procedure =
        create_mock_notes_procedure(tx_context.expected_output_notes());

    let code = format!(
        "
        use.kernel::prologue
        use.test::account
        use.kernel::epilogue

        {output_notes_data_procedure}

        begin
            exec.prologue::prepare_transaction
            exec.create_mock_notes
            exec.epilogue::finalize_transaction
            
            # truncate the stack
            movupw.3 dropw movupw.3 dropw movup.9 drop
        end
        "
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );
    assert_execution_error!(process, ERR_EPILOGUE_TOTAL_NUMBER_OF_ASSETS_MUST_STAY_THE_SAME);
    Ok(())
}

#[test]
fn test_epilogue_asset_preservation_violation_too_many_fungible_input() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder
        .add_existing_mock_account_with_assets(Auth::IncrNonce, AssetVault::mock().assets())?;
    let mock_chain = builder.build()?;

    let fungible_asset_1: Asset = FungibleAsset::new(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1.try_into()?,
        CONSUMED_ASSET_1_AMOUNT,
    )?
    .into();
    let fungible_asset_2: Asset = FungibleAsset::new(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into()?,
        CONSUMED_ASSET_2_AMOUNT,
    )?
    .into();
    let fungible_asset_3: Asset = FungibleAsset::new(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3.try_into()?,
        CONSUMED_ASSET_3_AMOUNT,
    )?
    .into();

    let output_note_1 = NoteBuilder::new(account.id(), rng())
        .add_assets([fungible_asset_1])
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;
    let output_note_2 = NoteBuilder::new(account.id(), rng())
        .add_assets([fungible_asset_2])
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;
    let output_note_3 = NoteBuilder::new(account.id(), rng())
        .add_assets([fungible_asset_3])
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;

    let input_note = create_spawn_note(
        ACCOUNT_ID_SENDER.try_into()?,
        vec![&output_note_1, &output_note_2, &output_note_3],
    )?;

    let tx_context = mock_chain
        .build_tx_context(TxContextInput::AccountId(account.id()), &[], &[input_note])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(output_note_1),
            OutputNote::Full(output_note_2),
        ])
        .build()?;

    let output_notes_data_procedure =
        create_mock_notes_procedure(tx_context.expected_output_notes());

    let code = format!(
        "
        use.kernel::prologue
        use.test::account
        use.kernel::epilogue

        {output_notes_data_procedure}

        begin
            exec.prologue::prepare_transaction
            exec.create_mock_notes
            exec.epilogue::finalize_transaction
                        
            # truncate the stack
            movupw.3 dropw movupw.3 dropw movup.9 drop
        end
        "
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_EPILOGUE_TOTAL_NUMBER_OF_ASSETS_MUST_STAY_THE_SAME);
    Ok(())
}

#[test]
fn test_block_expiration_height_monotonically_decreases() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let test_pairs: [(u64, u64); 3] = [(9, 12), (18, 3), (20, 20)];
    let code_template = "
        use.kernel::prologue
        use.kernel::tx
        use.kernel::epilogue
        use.kernel::account

        begin
            exec.prologue::prepare_transaction
            push.{value_1}
            exec.tx::update_expiration_block_num
            push.{value_2}
            exec.tx::update_expiration_block_num

            push.{min_value} exec.tx::get_expiration_delta assert_eq

            exec.epilogue::finalize_transaction
                        
            # truncate the stack
            movupw.3 dropw movupw.3 dropw movup.9 drop
        end
        ";

    for (v1, v2) in test_pairs {
        let code = &code_template
            .replace("{value_1}", &v1.to_string())
            .replace("{value_2}", &v2.to_string())
            .replace("{min_value}", &v2.min(v1).to_string());

        let process = &tx_context.execute_code_with_assembler(
            code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )?;
        let process_state: ProcessState = process.into();

        // Expiry block should be set to transaction's block + the stored expiration delta
        // (which can only decrease, not increase)
        let expected_expiry =
            v1.min(v2) + tx_context.tx_inputs().block_header().block_num().as_u64();
        assert_eq!(process_state.get_stack_item(8).as_int(), expected_expiry);
    }

    Ok(())
}

#[test]
fn test_invalid_expiration_deltas() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let test_values = [0u64, u16::MAX as u64 + 1, u32::MAX as u64];
    let code_template = "
        use.kernel::tx

        begin
            push.{value_1}
            exec.tx::update_expiration_block_num
        end
        ";

    for value in test_values {
        let code = &code_template.replace("{value_1}", &value.to_string());
        let process = tx_context.execute_code_with_assembler(
            code,
            TransactionKernel::testing_assembler_with_mock_account(),
        );

        assert_execution_error!(process, ERR_TX_INVALID_EXPIRATION_DELTA);
    }

    Ok(())
}

#[test]
fn test_no_expiration_delta_set() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let code_template = "
    use.kernel::prologue
    use.kernel::epilogue
    use.kernel::tx
    use.kernel::account

    begin
        exec.prologue::prepare_transaction

        exec.tx::get_expiration_delta assertz

        exec.epilogue::finalize_transaction
                    
        # truncate the stack
        movupw.3 dropw movupw.3 dropw movup.9 drop
    end
    ";

    let process = &tx_context.execute_code_with_assembler(
        code_template,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;
    let process_state: ProcessState = process.into();

    // Default value should be equal to u32::max, set in the prologue
    assert_eq!(process_state.get_stack_item(8).as_int() as u32, u32::MAX);

    Ok(())
}

#[test]
fn test_epilogue_increment_nonce_success() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let expected_nonce = ONE + ONE;

    let code = format!(
        "
        use.kernel::prologue
        use.test::account
        use.kernel::epilogue
        use.kernel::memory

        begin
            exec.prologue::prepare_transaction

            push.1.2.3.4
            push.0
            call.account::set_item
            dropw

            exec.epilogue::finalize_transaction

            # clean the stack
            dropw dropw dropw dropw

            exec.memory::get_acct_nonce
            push.{expected_nonce} assert_eq
        end
        "
    );

    tx_context.execute_code_with_assembler(
        code.as_str(),
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;
    Ok(())
}

#[test]
fn test_epilogue_increment_nonce_violation() -> anyhow::Result<()> {
    let tx_context = {
        let output_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into()?, &[FungibleAsset::mock(100)]);
        let input_note_1 = create_spawn_note(ACCOUNT_ID_SENDER.try_into()?, vec![&output_note_1])?;
        TransactionContextBuilder::with_noop_auth_account(ONE)
            .extend_input_notes(vec![input_note_1])
            .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
            .build()?
    };

    let output_notes_data_procedure =
        create_mock_notes_procedure(tx_context.expected_output_notes());

    let code = format!(
        "
        use.kernel::prologue
        use.test::account
        use.kernel::epilogue

        {output_notes_data_procedure}

        begin
            exec.prologue::prepare_transaction

            exec.create_mock_notes

            push.91.92.93.94
            push.0
            call.account::set_item
            dropw

            exec.epilogue::finalize_transaction

            # truncate the stack
            movupw.3 dropw movupw.3 dropw movup.9 drop
        end
        "
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );
    assert_execution_error!(process, ERR_ACCOUNT_NONCE_DID_NOT_INCREASE_AFTER_STATE_CHANGE);

    Ok(())
}

#[test]
fn test_epilogue_execute_empty_transaction() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_noop_auth_account(ONE).build()?;

    let err = tx_context.execute().expect_err("Expected execution to fail");
    let TransactionExecutorError::TransactionProgramExecutionFailed(err) = err else {
        panic!("unexpected error")
    };

    assert_execution_error!(Err::<(), _>(err), ERR_EPILOGUE_EXECUTED_TRANSACTION_IS_EMPTY);

    Ok(())
}

#[test]
fn test_epilogue_empty_transaction_with_empty_output_note() -> anyhow::Result<()> {
    let tag =
        NoteTag::from_account_id(ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE.try_into()?);
    let aux = Felt::new(26);
    let note_type = NoteType::Private;

    // create an empty output note in the transaction script
    let tx_script_source = format!(
        r#"
        use.miden::tx
        use.kernel::prologue
        use.kernel::epilogue
        use.kernel::note

        begin
            exec.prologue::prepare_transaction

            # prepare the values for note creation
            push.1.2.3.4      # recipient
            push.1            # note_execution_hint (NoteExecutionHint::Always)
            push.{note_type}  # note_type
            push.{aux}        # aux
            push.{tag}        # tag
            # => [tag, aux, note_type, note_execution_hint, RECIPIENT]

            # pad the stack with zeros before calling the `create_note`.
            padw padw swapdw
            # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8)]

            # create the note
            call.tx::create_note
            # => [note_idx, GARBAGE(15)]

            # make sure that output note was created: compare the output note hash with an empty 
            # word
            exec.note::compute_output_notes_commitment
            padw eqw assertz.err="output note was created, but the output notes hash remains to be zeros"

            # clean the stack
            dropw dropw dropw dropw

            exec.epilogue::finalize_transaction
        end
    "#,
        note_type = note_type as u8,
    );

    let tx_context = TransactionContextBuilder::with_noop_auth_account(ONE).build()?;

    let result = tx_context.execute_code(&tx_script_source).map(|_| ());

    // assert that even if the output note was created, the transaction is considered empty
    assert_execution_error!(result, ERR_EPILOGUE_EXECUTED_TRANSACTION_IS_EMPTY);

    Ok(())
}
