use alloc::{collections::BTreeSet, string::String, vec::Vec};
use std::sync::Arc;

use anyhow::Context;
use miden_lib::{
    errors::tx_kernel_errors::{
        ERR_NON_FUNGIBLE_ASSET_ALREADY_EXISTS, ERR_TX_NUMBER_OF_OUTPUT_NOTES_EXCEEDS_LIMIT,
    },
    transaction::{
        TransactionKernel,
        memory::{
            NOTE_MEM_SIZE, NUM_OUTPUT_NOTES_PTR, OUTPUT_NOTE_ASSETS_OFFSET,
            OUTPUT_NOTE_METADATA_OFFSET, OUTPUT_NOTE_RECIPIENT_OFFSET, OUTPUT_NOTE_SECTION_OFFSET,
        },
    },
    utils::word_to_masm_push_string,
};
use miden_objects::{
    Digest, FieldElement,
    account::{Account, AccountId},
    assembly::diagnostics::{IntoDiagnostic, NamedSource, miette},
    asset::{Asset, FungibleAsset, NonFungibleAsset},
    block::BlockNumber,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteExecutionMode, NoteHeader, NoteId, NoteInputs,
        NoteMetadata, NoteRecipient, NoteScript, NoteTag, NoteType,
    },
    testing::{
        account_component::IncrNonceAuthComponent,
        account_id::{
            ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET, ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PRIVATE_SENDER, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2, ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER,
        },
        constants::{FUNGIBLE_ASSET_AMOUNT, NON_FUNGIBLE_ASSET_DATA, NON_FUNGIBLE_ASSET_DATA_2},
        note::DEFAULT_NOTE_CODE,
    },
    transaction::{InputNotes, OutputNote, OutputNotes, TransactionArgs, TransactionScript},
};
use miden_tx::{
    TransactionExecutor, TransactionExecutorError, TransactionHost, TransactionMastStore,
    host::ScriptMastForestStore,
};
use vm_processor::MemAdviceProvider;

use super::{Felt, ONE, ProcessState, Word, ZERO};
use crate::{
    Auth, MockChain, TransactionContextBuilder, assert_execution_error,
    kernel_tests::tx::read_root_mem_word, utils::create_p2any_note,
};

#[test]
fn test_fpi_anchoring_validations() -> anyhow::Result<()> {
    // Create a chain with an account
    let mut mock_chain = MockChain::new();
    let account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    mock_chain.prove_next_block()?;

    // Retrieve inputs which will become stale
    let inputs = mock_chain
        .get_foreign_account_inputs(account.id())
        .expect("failed to get foreign account inputs");

    // Add account to modify account tree
    let new_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    mock_chain.prove_next_block()?;

    // Attempt to execute with older foreign account inputs
    let transaction = mock_chain
        .build_tx_context(new_account.id(), &[], &[])?
        .foreign_accounts(vec![inputs])
        .build()?
        .execute();

    assert_matches::assert_matches!(
        transaction,
        Err(TransactionExecutorError::ForeignAccountNotAnchoredInReference(_))
    );
    Ok(())
}

#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn test_future_input_note_fails() -> anyhow::Result<()> {
    // Create a chain with an account
    let mut mock_chain = MockChain::new();
    let account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    mock_chain.prove_until_block(10u32)?;

    // Create note that will land on a future block
    let note = mock_chain
        .add_pending_p2id_note(
            account.id(),
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
            &[],
            NoteType::Private,
        )
        .unwrap();
    mock_chain.prove_next_block()?;

    // Get as input note, and assert that the note was created after block 1 (which we'll
    // use as reference)
    let input_note = mock_chain.get_public_note(&note.id()).expect("note not found");
    assert!(input_note.location().unwrap().block_num() > 1.into());

    mock_chain.prove_next_block()?;
    mock_chain.prove_next_block()?;

    // Attempt to execute with a note created in the future
    let tx_context = mock_chain.build_tx_context(account.id(), &[], &[])?.build()?;
    let source_manager = tx_context.source_manager();

    let tx_executor = TransactionExecutor::new(&tx_context, None);
    // Try to execute with block_ref==1
    let error = tx_executor.execute_transaction(
        account.id(),
        BlockNumber::from(1),
        InputNotes::new(vec![input_note]).unwrap(),
        TransactionArgs::default(),
        source_manager,
    );

    assert_matches::assert_matches!(
        error,
        Err(TransactionExecutorError::NoteBlockPastReferenceBlock(..))
    );

    Ok(())
}

#[test]
fn test_create_note() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;
    let account_id = tx_context.account().id();

    let recipient = [ZERO, ONE, Felt::new(2), Felt::new(3)];
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(account_id);

    let code = format!(
        "
        use.miden::tx
        
        use.kernel::prologue

        begin
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{note_execution_hint}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note

            # truncate the stack
            swapdw dropw dropw
        end
        ",
        recipient = word_to_masm_push_string(&recipient),
        PUBLIC_NOTE = NoteType::Public as u8,
        note_execution_hint = Felt::from(NoteExecutionHint::after_block(23.into()).unwrap()),
        tag = tag,
    );

    let process = &tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    assert_eq!(
        read_root_mem_word(&process.into(), NUM_OUTPUT_NOTES_PTR),
        [ONE, ZERO, ZERO, ZERO],
        "number of output notes must increment by 1",
    );

    assert_eq!(
        read_root_mem_word(
            &process.into(),
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_RECIPIENT_OFFSET
        ),
        recipient,
        "recipient must be stored at the correct memory location",
    );

    let expected_note_metadata: Word = NoteMetadata::new(
        account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::after_block(23.into())?,
        Felt::new(27),
    )?
    .into();

    assert_eq!(
        read_root_mem_word(
            &process.into(),
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET
        ),
        [
            expected_note_metadata[0],
            expected_note_metadata[1],
            expected_note_metadata[2],
            expected_note_metadata[3]
        ],
        "metadata must be stored at the correct memory location",
    );

    assert_eq!(
        process.stack.get(0),
        ZERO,
        "top item on the stack is the index of the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_with_invalid_tag() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let invalid_tag = Felt::new((NoteType::Public as u64) << 62);
    let valid_tag: Felt = NoteTag::for_local_use_case(0, 0).unwrap().into();

    // Test invalid tag
    assert!(
        tx_context
            .execute_code_with_assembler(
                &note_creation_script(invalid_tag),
                TransactionKernel::testing_assembler()
            )
            .is_err()
    );
    // Test valid tag
    assert!(
        tx_context
            .execute_code_with_assembler(
                &note_creation_script(valid_tag),
                TransactionKernel::testing_assembler()
            )
            .is_ok()
    );
    Ok(())
}

fn note_creation_script(tag: Felt) -> String {
    format!(
        "
            use.miden::tx
            use.kernel::prologue
    
            begin
                exec.prologue::prepare_transaction
    
                push.{recipient}
                push.{execution_hint_always}
                push.{PUBLIC_NOTE}
                push.{aux}
                push.{tag}
    
                call.tx::create_note

                # clean the stack
                dropw dropw
            end
            ",
        recipient = word_to_masm_push_string(&[ZERO, ONE, Felt::new(2), Felt::new(3)]),
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        PUBLIC_NOTE = NoteType::Public as u8,
        aux = Felt::ZERO,
    )
}

#[test]
fn test_create_note_too_many_notes() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let code = format!(
        "
        use.miden::tx
        use.kernel::constants
        use.kernel::memory
        use.kernel::prologue

        begin
            exec.constants::get_max_num_output_notes
            exec.memory::set_num_output_notes
            exec.prologue::prepare_transaction
            
            push.{recipient}
            push.{execution_hint_always}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
        end
        ",
        tag = NoteTag::for_local_use_case(1234, 5678).unwrap(),
        recipient = word_to_masm_push_string(&[ZERO, ONE, Felt::new(2), Felt::new(3)]),
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        PUBLIC_NOTE = NoteType::Public as u8,
        aux = Felt::ZERO,
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_TX_NUMBER_OF_OUTPUT_NOTES_EXCEEDS_LIMIT);
    Ok(())
}

#[test]
fn test_get_output_notes_commitment() -> anyhow::Result<()> {
    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );

        let output_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into()?, &[FungibleAsset::mock(100)]);

        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_PRIVATE_SENDER.try_into()?, &[FungibleAsset::mock(100)]);

        let input_note_2 =
            create_p2any_note(ACCOUNT_ID_PRIVATE_SENDER.try_into()?, &[FungibleAsset::mock(200)]);

        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2])
            .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
            .build()?
    };

    // extract input note data
    let input_note_1 = tx_context.tx_inputs().input_notes().get_note(0).note();
    let input_asset_1 = **input_note_1
        .assets()
        .iter()
        .take(1)
        .collect::<Vec<_>>()
        .first()
        .context("getting first expected input asset")?;
    let input_note_2 = tx_context.tx_inputs().input_notes().get_note(1).note();
    let input_asset_2 = **input_note_2
        .assets()
        .iter()
        .take(1)
        .collect::<Vec<_>>()
        .first()
        .context("getting second expected input asset")?;

    // Choose random accounts as the target for the note tag.
    let network_account = AccountId::try_from(ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET)?;
    let local_account = AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET)?;

    // create output note 1
    let output_serial_no_1 = [Felt::new(8); 4];
    let output_tag_1 = NoteTag::from_account_id(network_account);
    let assets = NoteAssets::new(vec![input_asset_1])?;
    let metadata = NoteMetadata::new(
        tx_context.tx_inputs().account().id(),
        NoteType::Public,
        output_tag_1,
        NoteExecutionHint::Always,
        ZERO,
    )?;
    let inputs = NoteInputs::new(vec![])?;
    let recipient = NoteRecipient::new(output_serial_no_1, input_note_1.script().clone(), inputs);
    let output_note_1 = Note::new(assets, metadata, recipient);

    // create output note 2
    let output_serial_no_2 = [Felt::new(11); 4];
    let output_tag_2 = NoteTag::from_account_id(local_account);
    let assets = NoteAssets::new(vec![input_asset_2])?;
    let metadata = NoteMetadata::new(
        tx_context.tx_inputs().account().id(),
        NoteType::Public,
        output_tag_2,
        NoteExecutionHint::after_block(123.into())?,
        ZERO,
    )?;
    let inputs = NoteInputs::new(vec![])?;
    let recipient = NoteRecipient::new(output_serial_no_2, input_note_2.script().clone(), inputs);
    let output_note_2 = Note::new(assets, metadata, recipient);

    // compute expected output notes commitment
    let expected_output_notes_commitment = OutputNotes::new(vec![
        OutputNote::Full(output_note_1.clone()),
        OutputNote::Full(output_note_2.clone()),
    ])?
    .commitment();

    let code = format!(
        "
        use.std::sys

        use.miden::tx

        use.kernel::prologue
        use.test::account

        begin
            # => [BH, acct_id, IAH, NC]
            exec.prologue::prepare_transaction
            # => []

            # create output note 1
            push.{recipient_1}
            push.{NOTE_EXECUTION_HINT_1}
            push.{PUBLIC_NOTE}
            push.{aux_1}
            push.{tag_1}
            call.tx::create_note
            # => [note_idx]

            push.{asset_1}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]
            
            dropw drop
            # => []

            # create output note 2
            push.{recipient_2}
            push.{NOTE_EXECUTION_HINT_2}
            push.{PUBLIC_NOTE}
            push.{aux_2}
            push.{tag_2}
            call.tx::create_note
            # => [note_idx]

            push.{asset_2} 
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw drop
            # => []

            # compute the output notes commitment
            exec.tx::get_output_notes_commitment
            # => [OUTPUT_NOTES_COMMITMENT]

            # truncate the stack
            exec.sys::truncate_stack
            # => [OUTPUT_NOTES_COMMITMENT]
        end
        ",
        PUBLIC_NOTE = NoteType::Public as u8,
        NOTE_EXECUTION_HINT_1 = Felt::from(output_note_1.metadata().execution_hint()),
        recipient_1 = word_to_masm_push_string(&output_note_1.recipient().digest()),
        tag_1 = output_note_1.metadata().tag(),
        aux_1 = output_note_1.metadata().aux(),
        asset_1 = word_to_masm_push_string(&Word::from(
            **output_note_1.assets().iter().take(1).collect::<Vec<_>>().first().unwrap()
        )),
        recipient_2 = word_to_masm_push_string(&output_note_2.recipient().digest()),
        NOTE_EXECUTION_HINT_2 = Felt::from(output_note_2.metadata().execution_hint()),
        tag_2 = output_note_2.metadata().tag(),
        aux_2 = output_note_2.metadata().aux(),
        asset_2 = word_to_masm_push_string(&Word::from(
            **output_note_2.assets().iter().take(1).collect::<Vec<_>>().first().unwrap()
        )),
    );

    let process = &tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;
    let process_state: ProcessState = process.into();

    assert_eq!(
        read_root_mem_word(&process_state, NUM_OUTPUT_NOTES_PTR),
        [Felt::new(2), ZERO, ZERO, ZERO],
        "The test creates two notes",
    );
    assert_eq!(
        NoteMetadata::try_from(read_root_mem_word(
            &process_state,
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET
        ))
        .unwrap(),
        *output_note_1.metadata(),
        "Validate the output note 1 metadata",
    );
    assert_eq!(
        NoteMetadata::try_from(read_root_mem_word(
            &process_state,
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET + NOTE_MEM_SIZE
        ))
        .unwrap(),
        *output_note_2.metadata(),
        "Validate the output note 1 metadata",
    );

    assert_eq!(process_state.get_stack_word(0), *expected_output_notes_commitment);
    Ok(())
}

#[test]
fn test_create_note_and_add_asset() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let recipient = [ZERO, ONE, Felt::new(2), Felt::new(3)];
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_id);
    let asset = [Felt::new(10), ZERO, faucet_id.suffix(), faucet_id.prefix().as_felt()];

    let code = format!(
        "
        use.miden::tx

        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction

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

    let process = &tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;
    let process_state: ProcessState = process.into();

    assert_eq!(
        read_root_mem_word(&process_state, OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET),
        asset,
        "asset must be stored at the correct memory location",
    );

    assert_eq!(
        process_state.get_stack_item(0),
        ZERO,
        "top item on the stack is the index to the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_and_add_multiple_assets() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let faucet = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let faucet_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2)?;

    let recipient = [ZERO, ONE, Felt::new(2), Felt::new(3)];
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_2);

    let asset = [Felt::new(10), ZERO, faucet.suffix(), faucet.prefix().as_felt()];
    let asset_2 = [Felt::new(20), ZERO, faucet_2.suffix(), faucet_2.prefix().as_felt()];
    let asset_3 = [Felt::new(30), ZERO, faucet_2.suffix(), faucet_2.prefix().as_felt()];
    let asset_2_and_3 = [Felt::new(50), ZERO, faucet_2.suffix(), faucet_2.prefix().as_felt()];

    let non_fungible_asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);
    let non_fungible_asset_encoded = Word::from(non_fungible_asset);

    let code = format!(
        "
        use.miden::tx

        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx]

            push.{asset}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{asset_2}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{asset_3}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{nft}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            # truncate the stack
            swapdw dropw drop drop drop
        end
        ",
        recipient = word_to_masm_push_string(&recipient),
        PUBLIC_NOTE = NoteType::Public as u8,
        tag = tag,
        asset = word_to_masm_push_string(&asset),
        asset_2 = word_to_masm_push_string(&asset_2),
        asset_3 = word_to_masm_push_string(&asset_3),
        nft = word_to_masm_push_string(&non_fungible_asset_encoded),
    );

    let process = &tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;
    let process_state: ProcessState = process.into();

    assert_eq!(
        read_root_mem_word(&process_state, OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET),
        asset,
        "asset must be stored at the correct memory location",
    );

    assert_eq!(
        read_root_mem_word(
            &process_state,
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET + 4
        ),
        asset_2_and_3,
        "asset_2 and asset_3 must be stored at the same correct memory location",
    );

    assert_eq!(
        read_root_mem_word(
            &process_state,
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET + 8
        ),
        Word::from(non_fungible_asset_encoded),
        "non_fungible_asset must be stored at the correct memory location",
    );

    assert_eq!(
        process_state.get_stack_item(0),
        ZERO,
        "top item on the stack is the index to the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_and_add_same_nft_twice() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let recipient = [ZERO, ONE, Felt::new(2), Felt::new(3)];
    let tag = NoteTag::for_public_use_case(999, 777, NoteExecutionMode::Local).unwrap();
    let non_fungible_asset = NonFungibleAsset::mock(&[1, 2, 3]);
    let encoded = Word::from(non_fungible_asset);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account
        use.miden::tx

        begin
            exec.prologue::prepare_transaction
            # => []

            padw padw
            push.{recipient}
            push.{execution_hint_always}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx, pad(15)]

            push.{nft} 
            call.tx::add_asset_to_note
            # => [NFT, note_idx, pad(15)]
            dropw

            push.{nft} 
            call.tx::add_asset_to_note
            # => [NFT, note_idx, pad(15)]

            repeat.5 dropw end
        end
        ",
        recipient = word_to_masm_push_string(&recipient),
        PUBLIC_NOTE = NoteType::Public as u8,
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        aux = Felt::new(0),
        tag = tag,
        nft = word_to_masm_push_string(&encoded),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_NON_FUNGIBLE_ASSET_ALREADY_EXISTS);
    Ok(())
}

#[test]
fn test_build_recipient_hash() -> anyhow::Result<()> {
    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );

        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1])
            .build()?
    };
    let input_note_1 = tx_context.tx_inputs().input_notes().get_note(0).note();

    // create output note
    let output_serial_no = [ZERO, ONE, Felt::new(2), Felt::new(3)];
    let aux = Felt::new(27);
    let tag = NoteTag::for_public_use_case(42, 42, NoteExecutionMode::Network).unwrap();
    let single_input = 2;
    let inputs = NoteInputs::new(vec![Felt::new(single_input)]).unwrap();
    let input_commitment = inputs.commitment();

    let recipient = NoteRecipient::new(output_serial_no, input_note_1.script().clone(), inputs);
    let code = format!(
        "
        use.miden::tx
        use.kernel::prologue

        proc.build_recipient_hash
            exec.tx::build_recipient_hash
        end

        begin
            exec.prologue::prepare_transaction

            # pad the stack before call
            padw

            # input
            push.{input_commitment}
            # SCRIPT_ROOT
            push.{script_root}
            # SERIAL_NUM
            push.{output_serial_no}
            # => [SERIAL_NUM, SCRIPT_ROOT, INPUT_COMMITMENT, pad(4)]

            call.build_recipient_hash
            # => [RECIPIENT, pad(12)]

            push.{execution_hint}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}
            # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(12)]

            call.tx::create_note
            # => [note_idx, pad(19)]

            # clean the stack
            dropw dropw dropw dropw dropw
        end
        ",
        script_root = input_note_1.script().clone().root(),
        output_serial_no = word_to_masm_push_string(&output_serial_no),
        PUBLIC_NOTE = NoteType::Public as u8,
        tag = tag,
        execution_hint = Felt::from(NoteExecutionHint::after_block(2.into()).unwrap()),
        aux = aux,
    );

    let process = &tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    assert_eq!(
        read_root_mem_word(&process.into(), NUM_OUTPUT_NOTES_PTR),
        [ONE, ZERO, ZERO, ZERO],
        "number of output notes must increment by 1",
    );

    let recipient_digest: Vec<Felt> = recipient.clone().digest().to_vec();

    assert_eq!(
        read_root_mem_word(
            &process.into(),
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_RECIPIENT_OFFSET
        ),
        recipient_digest.as_slice(),
        "recipient hash not correct",
    );
    Ok(())
}

// BLOCK TESTS
// ================================================================================================

#[test]
fn test_block_procedures() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let code = "
        use.miden::tx
        use.kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # get the block data
            exec.tx::get_block_number
            exec.tx::get_block_timestamp
            exec.tx::get_block_commitment
            # => [BLOCK_COMMITMENT, block_timestamp, block_number]

            # truncate the stack
            swapdw dropw dropw
        end
        ";

    let process = &tx_context.execute_code_with_assembler(
        code,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    assert_eq!(
        process.stack.get_word(0),
        tx_context.tx_inputs().block_header().commitment().as_elements(),
        "top word on the stack should be equal to the block header commitment"
    );

    assert_eq!(
        process.stack.get(4).as_int(),
        tx_context.tx_inputs().block_header().timestamp() as u64,
        "fifth element on the stack should be equal to the timestamp of the last block creation"
    );

    assert_eq!(
        process.stack.get(5).as_int(),
        tx_context.tx_inputs().block_header().block_num().as_u64(),
        "sixth element on the stack should be equal to the block number"
    );
    Ok(())
}

/// Tests that the transaction witness retrieved from an executed transaction contains all necessary
/// advice input to execute the transaction again.
#[test]
fn advice_inputs_from_transaction_witness_are_sufficient_to_reexecute_transaction() -> miette::Result<()> {
    // Creates a mockchain with an account and a note that it can consume
    let tx_context = {
        let mut mock_chain = MockChain::new();
        let account = mock_chain.add_pending_existing_wallet(crate::Auth::BasicAuth, vec![]);
        let p2id_note = mock_chain
            .add_pending_p2id_note(
                ACCOUNT_ID_SENDER.try_into().unwrap(),
                account.id(),
                &[FungibleAsset::mock(100)],
                NoteType::Public,
            )
            .unwrap();
        mock_chain.prove_next_block().unwrap();

        mock_chain
            .build_tx_context(account.id(), &[], &[p2id_note])
            .unwrap()
            .build()
            .unwrap()
    };

    let source_manager = tx_context.source_manager();
    let executed_transaction = tx_context.execute().into_diagnostic()?;

    let tx_inputs = executed_transaction.tx_inputs();
    let tx_args = executed_transaction.tx_args();

    let scripts_mast_store = ScriptMastForestStore::new(
        tx_args.tx_script(),
        tx_inputs.input_notes().iter().map(|n| n.note().script()),
    );

    // use the witness to execute the transaction again
    let (stack_inputs, advice_inputs) = TransactionKernel::prepare_inputs(
        tx_inputs,
        tx_args,
        Some(executed_transaction.advice_witness().clone()),
    );

    let mem_advice_provider = MemAdviceProvider::from(advice_inputs.into_inner());

    // load account/note/tx_script MAST to the mast_store
    let mast_store = Arc::new(TransactionMastStore::new());
    mast_store.load_account_code(tx_inputs.account().code());

    let mut host: TransactionHost<MemAdviceProvider> = TransactionHost::new(
        &tx_inputs.account().into(),
        mem_advice_provider,
        mast_store.as_ref(),
        scripts_mast_store,
        None,
        BTreeSet::new(),
    )
    .unwrap();
    let result = vm_processor::execute(
        &TransactionKernel::main(),
        stack_inputs,
        &mut host,
        Default::default(),
        source_manager,
    )?;

    let (advice_provider, _, output_notes, _signatures, _tx_progress) = host.into_parts();
    let (_, map, _) = advice_provider.into_parts();
    let tx_outputs = TransactionKernel::from_transaction_parts(
        result.stack_outputs(),
        &map.into(),
        output_notes,
    )
    .unwrap();

    assert_eq!(
        executed_transaction.final_account().commitment(),
        tx_outputs.account.commitment()
    );
    assert_eq!(executed_transaction.output_notes(), &tx_outputs.output_notes);

    Ok(())
}

#[test]
fn executed_transaction_output_notes() -> anyhow::Result<()> {
    let assembler = TransactionKernel::testing_assembler();
    let auth_component = IncrNonceAuthComponent::new(assembler.clone())?;

    let executor_account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        Felt::ONE,
        auth_component,
        assembler,
    );
    let account_id = executor_account.id();

    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);

    let combined_asset = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let removed_asset_4 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT / 2,
        )
        .expect("asset is valid"),
    );

    let tag1 = NoteTag::from_account_id(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
    );
    let tag2 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let tag3 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let aux1 = Felt::new(27);
    let aux2 = Felt::new(28);
    let aux3 = Felt::new(29);

    let note_type1 = NoteType::Private;
    let note_type2 = NoteType::Public;
    let note_type3 = NoteType::Public;

    tag1.validate(note_type1).expect("note tag 1 should support private notes");
    tag2.validate(note_type2).expect("note tag 2 should support public notes");
    tag3.validate(note_type3).expect("note tag 3 should support public notes");

    // In this test we create 3 notes. Note 1 is private, Note 2 is public and Note 3 is public
    // without assets.

    // Create the expected output note for Note 2 which is public
    let serial_num_2 = Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);
    let note_script_2 =
        NoteScript::compile(DEFAULT_NOTE_CODE, TransactionKernel::testing_assembler())?;
    let inputs_2 = NoteInputs::new(vec![ONE])?;
    let metadata_2 =
        NoteMetadata::new(account_id, note_type2, tag2, NoteExecutionHint::none(), aux2)?;
    let vault_2 = NoteAssets::new(vec![removed_asset_3, removed_asset_4])?;
    let recipient_2 = NoteRecipient::new(serial_num_2, note_script_2, inputs_2);
    let expected_output_note_2 = Note::new(vault_2, metadata_2, recipient_2);

    // Create the expected output note for Note 3 which is public
    let serial_num_3 = Word::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]);
    let note_script_3 =
        NoteScript::compile(DEFAULT_NOTE_CODE, TransactionKernel::testing_assembler())?;
    let inputs_3 = NoteInputs::new(vec![ONE, Felt::new(2)])?;
    let metadata_3 = NoteMetadata::new(
        account_id,
        note_type3,
        tag3,
        NoteExecutionHint::on_block_slot(1, 2, 3),
        aux3,
    )?;
    let vault_3 = NoteAssets::new(vec![])?;
    let recipient_3 = NoteRecipient::new(serial_num_3, note_script_3, inputs_3);
    let expected_output_note_3 = Note::new(vault_3, metadata_3, recipient_3);

    let tx_script_src = format!(
        "\
        use.miden::contracts::wallets::basic->wallet
        use.miden::tx
        use.test::account

        # Inputs:  [tag, aux, note_type, execution_hint, RECIPIENT]
        # Outputs: [note_idx]
        proc.create_note
            # pad the stack before the call to prevent accidental modification of the deeper stack
            # elements
            padw padw swapdw
            # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8)]

            call.tx::create_note
            # => [note_idx, pad(15)]

            # remove excess PADs from the stack
            swapdw dropw dropw movdn.7 dropw drop drop drop
            # => [note_idx]
        end

        # Inputs:  [ASSET, note_idx]
        # Outputs: [ASSET, note_idx]
        proc.move_asset_to_note
            # pad the stack before call
            push.0.0.0 movdn.7 movdn.7 movdn.7 padw padw swapdw
            # => [ASSET, note_idx, pad(11)]

            call.wallet::move_asset_to_note
            # => [ASSET, note_idx, pad(11)]

            # remove excess PADs from the stack
            swapdw dropw dropw swapw movdn.7 drop drop drop
            # => [ASSET, note_idx]
        end

        ## TRANSACTION SCRIPT
        ## ========================================================================================
        begin
            ## Send some assets from the account vault
            ## ------------------------------------------------------------------------------------
            # partially deplete fungible asset balance
            push.0.1.2.3                        # recipient
            push.{EXECUTION_HINT_1}             # note execution hint
            push.{NOTETYPE1}                    # note_type
            push.{aux1}                         # aux
            push.{tag1}                         # tag
            exec.create_note
            # => [note_idx]

            push.{REMOVED_ASSET_1}              # asset_1
            # => [ASSET, note_idx]

            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_2}              # asset_2
            exec.move_asset_to_note dropw drop
            # => []

            # send non-fungible asset
            push.{RECIPIENT2}                   # recipient
            push.{EXECUTION_HINT_2}             # note execution hint
            push.{NOTETYPE2}                    # note_type
            push.{aux2}                         # aux
            push.{tag2}                         # tag
            exec.create_note
            # => [note_idx]

            push.{REMOVED_ASSET_3}              # asset_3
            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_4}              # asset_4
            exec.move_asset_to_note dropw drop
            # => []

            # create a public note without assets
            push.{RECIPIENT3}                   # recipient
            push.{EXECUTION_HINT_3}             # note execution hint
            push.{NOTETYPE3}                    # note_type
            push.{aux3}                         # aux
            push.{tag3}                         # tag
            exec.create_note drop
            # => []
        end
    ",
        REMOVED_ASSET_1 = word_to_masm_push_string(&Word::from(removed_asset_1)),
        REMOVED_ASSET_2 = word_to_masm_push_string(&Word::from(removed_asset_2)),
        REMOVED_ASSET_3 = word_to_masm_push_string(&Word::from(removed_asset_3)),
        REMOVED_ASSET_4 = word_to_masm_push_string(&Word::from(removed_asset_4)),
        RECIPIENT2 =
            word_to_masm_push_string(&Word::from(expected_output_note_2.recipient().digest())),
        RECIPIENT3 =
            word_to_masm_push_string(&Word::from(expected_output_note_3.recipient().digest())),
        NOTETYPE1 = note_type1 as u8,
        NOTETYPE2 = note_type2 as u8,
        NOTETYPE3 = note_type3 as u8,
        EXECUTION_HINT_1 = Felt::from(NoteExecutionHint::always()),
        EXECUTION_HINT_2 = Felt::from(NoteExecutionHint::none()),
        EXECUTION_HINT_3 = Felt::from(NoteExecutionHint::on_block_slot(11, 22, 33)),
    );

    let tx_script = TransactionScript::compile(
        tx_script_src,
        TransactionKernel::testing_assembler_with_mock_account().with_debug_mode(true),
    )?;

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness

    let tx_context = TransactionContextBuilder::new(executor_account)
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![
            OutputNote::Full(expected_output_note_2.clone()),
            OutputNote::Full(expected_output_note_3.clone()),
        ])
        .build()?;

    let executed_transaction = tx_context.execute()?;

    // output notes
    // --------------------------------------------------------------------------------------------
    let output_notes = executed_transaction.output_notes();

    // check the total number of notes
    assert_eq!(output_notes.num_notes(), 3);

    // assert that the expected output note 1 is present
    let resulting_output_note_1 = executed_transaction.output_notes().get_note(0);

    let expected_recipient_1 =
        Digest::from([Felt::new(0), Felt::new(1), Felt::new(2), Felt::new(3)]);
    let expected_note_assets_1 = NoteAssets::new(vec![combined_asset])?;
    let expected_note_id_1 = NoteId::new(expected_recipient_1, expected_note_assets_1.commitment());
    assert_eq!(resulting_output_note_1.id(), expected_note_id_1);

    // assert that the expected output note 2 is present
    let resulting_output_note_2 = executed_transaction.output_notes().get_note(1);

    let expected_note_id_2 = expected_output_note_2.id();
    let expected_note_metadata_2 = expected_output_note_2.metadata();
    assert_eq!(
        NoteHeader::from(resulting_output_note_2),
        NoteHeader::new(expected_note_id_2, *expected_note_metadata_2)
    );

    // assert that the expected output note 3 is present and has no assets
    let resulting_output_note_3 = executed_transaction.output_notes().get_note(2);

    assert_eq!(expected_output_note_3.id(), resulting_output_note_3.id());
    assert_eq!(expected_output_note_3.assets(), resulting_output_note_3.assets().unwrap());

    // make sure that the number of note inputs remains the same
    let resulting_note_2_recipient =
        resulting_output_note_2.recipient().expect("output note 2 is not full");
    assert_eq!(
        resulting_note_2_recipient.inputs().num_values(),
        expected_output_note_2.inputs().num_values()
    );

    let resulting_note_3_recipient =
        resulting_output_note_3.recipient().expect("output note 3 is not full");
    assert_eq!(
        resulting_note_3_recipient.inputs().num_values(),
        expected_output_note_3.inputs().num_values()
    );

    Ok(())
}

/// Tests that execute_tx_view_script returns the expected stack outputs.
#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn execute_tx_view_script() -> anyhow::Result<()> {
    let test_module_source = "
        export.foo
            push.3.4
            add
            swapw dropw
        end
    ";

    let source = NamedSource::new("test::module_1", test_module_source);
    let assembler = TransactionKernel::assembler();
    let source_manager = assembler.source_manager();
    let assembler = assembler
        .with_module(source)
        .map_err(|_| anyhow::anyhow!("adding source module"))?;

    let source = "
    use.test::module_1
    use.std::sys

    begin
        push.1.2
        call.module_1::foo
        exec.sys::truncate_stack
    end
    ";

    let tx_script = TransactionScript::compile(source, assembler)?;

    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script.clone())
        .build()?;
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let advice_inputs = tx_context.tx_args().advice_inputs().clone();

    let executor = TransactionExecutor::new(&tx_context, None);

    let stack_outputs = executor.execute_tx_view_script(
        account_id,
        block_ref,
        tx_script,
        advice_inputs,
        Vec::default(),
        source_manager,
    )?;

    assert_eq!(stack_outputs[..3], [Felt::new(7), Felt::new(2), ONE]);

    Ok(())
}

// TEST TRANSACTION SCRIPT
// ================================================================================================

/// Tests transaction script inputs.
#[test]
fn test_tx_script_inputs() -> anyhow::Result<()> {
    let tx_script_input_key = [Felt::new(9999), Felt::new(8888), Felt::new(9999), Felt::new(8888)];
    let tx_script_input_value = [Felt::new(9), Felt::new(8), Felt::new(7), Felt::new(6)];
    let tx_script_src = format!(
        "
        use.miden::account

        begin
            # push the tx script input key onto the stack
            push.{key}

            # load the tx script input value from the map and read it onto the stack
            adv.push_mapval adv_loadw

            # assert that the value is correct
            push.{value} assert_eqw
        end
        ",
        key = word_to_masm_push_string(&tx_script_input_key),
        value = word_to_masm_push_string(&tx_script_input_value)
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler()).unwrap();

    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script)
        .extend_advice_map([(tx_script_input_key, tx_script_input_value.into())])
        .build()?;

    tx_context.execute().context("failed to execute transaction")?;

    Ok(())
}

/// Tests transaction script arguments.
#[test]
fn test_tx_script_args() -> anyhow::Result<()> {
    let tx_script_arg = [Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)];

    let tx_script_src = r#"
        use.miden::account

        begin
            # => [TX_SCRIPT_ARG]
            # `TX_SCRIPT_ARG` value is a user provided word, which could be used during the
            # transaction execution. In this example it is a `[1, 2, 3, 4]` word.

            # assert the correctness of the argument
            dupw push.1.2.3.4 assert_eqw.err="provided transaction argument doesn't match the expected one"
            # => [TX_SCRIPT_ARG]

            # since we provided an advice map entry with the transaction script argument as a key,
            # we can obtain the value of this entry
            adv.push_mapval adv_push.4
            # => [[map_entry_values], TX_SCRIPT_ARG]

            # assert the correctness of the map entry values
            push.5.6.7.8 assert_eqw.err="obtained advice map value doesn't match the expected one"
        end"#;

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler())
            .context("failed to compile transaction script")?;

    // extend the advice map with the entry that is accessed using the provided transaction script
    // argument
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script)
        .extend_advice_map([(
            tx_script_arg,
            vec![Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)],
        )])
        .tx_script_arg(tx_script_arg)
        .build()?;

    tx_context.execute()?;

    Ok(())
}
