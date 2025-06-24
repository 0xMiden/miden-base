use alloc::{collections::BTreeMap, string::String, vec::Vec};

use anyhow::Context;
use miden_crypto::{
    dsa::rpo_falcon512::PublicKey,
    rand::{FeltRng, RpoRandomCoin},
};
use miden_lib::{
    account::{auth::RpoFalcon512, wallets::BasicWallet},
    errors::{
        MasmError, tx_kernel_errors::ERR_NOTE_ATTEMPT_TO_ACCESS_NOTE_SENDER_FROM_INCORRECT_CONTEXT,
    },
    transaction::{TransactionKernel, memory::CURRENT_INPUT_NOTE_PTR},
};
use miden_objects::{
    Digest, EMPTY_WORD, ONE, WORD_SIZE,
    account::{AccountBuilder, AccountComponent, AccountId},
    assembly::diagnostics::miette,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteExecutionMode, NoteInputs, NoteMetadata,
        NoteRecipient, NoteScript, NoteTag, NoteType,
    },
    testing::{
        account_id::{ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER},
        note::NoteBuilder,
    },
    transaction::{AccountInputs, OutputNote, TransactionArgs},
};
use miden_tx::TransactionExecutorError;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use vm_processor::{ProcessState, Word};

use super::{Felt, Process, ZERO, word_to_masm_push_string};
use crate::{
    Auth, MockChain, TransactionContext, TransactionContextBuilder, assert_execution_error,
    kernel_tests::tx::read_root_mem_word, utils::input_note_data_ptr,
};

#[test]
fn test_get_sender_no_sender() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();
    // calling get_sender should return sender
    let code = "
        use.kernel::memory
        use.kernel::prologue
        use.miden::note

        begin
            exec.prologue::prepare_transaction

            # force the current input note pointer to 0
            push.0 exec.memory::set_current_input_note_ptr

            # get the sender
            exec.note::get_sender
        end
        ";

    let process = tx_context.execute_code(code);

    assert_execution_error!(process, ERR_NOTE_ATTEMPT_TO_ACCESS_NOTE_SENDER_FROM_INCORRECT_CONTEXT);
}

#[test]
fn test_get_sender() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    // calling get_sender should return sender
    let code = "
        use.kernel::prologue
        use.kernel::note->note_internal
        use.miden::note

        begin
            exec.prologue::prepare_transaction
            exec.note_internal::prepare_note
            dropw dropw dropw dropw
            exec.note::get_sender

            # truncate the stack
            swapw dropw
        end
        ";

    let process = tx_context.execute_code(code).unwrap();

    let sender = tx_context.input_notes().get_note(0).note().metadata().sender();
    assert_eq!(process.stack.get(0), sender.prefix().as_felt());
    assert_eq!(process.stack.get(1), sender.suffix());
}

#[test]
fn test_get_vault_data() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let notes = tx_context.input_notes();

    // calling get_assets_info should return assets info
    let code = format!(
        "
        use.std::sys

        use.kernel::prologue
        use.kernel::note

        begin
            exec.prologue::prepare_transaction

            # get the assets info about note 0
            exec.note::get_assets_info

            # assert the assets data is correct
            push.{note_0_asset_commitment} assert_eqw
            push.{note_0_num_assets} assert_eq

            # increment current input note pointer
            exec.note::increment_current_input_note_ptr

            # get the assets info about note 1
            exec.note::get_assets_info

            # assert the assets data is correct
            push.{note_1_asset_commitment} assert_eqw
            push.{note_1_num_assets} assert_eq

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        note_0_asset_commitment =
            word_to_masm_push_string(&notes.get_note(0).note().assets().commitment()),
        note_0_num_assets = notes.get_note(0).note().assets().num_assets(),
        note_1_asset_commitment =
            word_to_masm_push_string(&notes.get_note(1).note().assets().commitment()),
        note_1_num_assets = notes.get_note(1).note().assets().num_assets(),
    );

    tx_context.execute_code(&code).unwrap();
}
#[test]
fn test_get_assets() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let notes = tx_context.input_notes();

    const DEST_POINTER_NOTE_0: u32 = 100000000;
    const DEST_POINTER_NOTE_1: u32 = 200000000;

    fn construct_asset_assertions(note: &Note) -> String {
        let mut code = String::new();
        for asset in note.assets().iter() {
            code += &format!(
                "
                # assert the asset is correct
                dup padw movup.4 mem_loadw push.{asset} assert_eqw push.4 add
                ",
                asset = word_to_masm_push_string(&<[Felt; 4]>::from(*asset))
            );
        }
        code
    }

    // calling get_assets should return assets at the specified address
    let code = format!(
        "
        use.std::sys

        use.kernel::prologue
        use.kernel::note->note_internal
        use.miden::note

        proc.process_note_0
            # drop the note inputs
            dropw dropw dropw dropw

            # set the destination pointer for note 0 assets
            push.{DEST_POINTER_NOTE_0}

            # get the assets
            exec.note::get_assets

            # assert the number of assets is correct
            eq.{note_0_num_assets} assert

            # assert the pointer is returned
            dup eq.{DEST_POINTER_NOTE_0} assert

            # asset memory assertions
            {NOTE_0_ASSET_ASSERTIONS}

            # clean pointer
            drop
        end

        proc.process_note_1
            # drop the note inputs
            dropw dropw dropw dropw

            # set the destination pointer for note 1 assets
            push.{DEST_POINTER_NOTE_1}

            # get the assets
            exec.note::get_assets

            # assert the number of assets is correct
            eq.{note_1_num_assets} assert

            # assert the pointer is returned
            dup eq.{DEST_POINTER_NOTE_1} assert

            # asset memory assertions
            {NOTE_1_ASSET_ASSERTIONS}

            # clean pointer
            drop
        end

        begin
            # prepare tx
            exec.prologue::prepare_transaction

            # prepare note 0
            exec.note_internal::prepare_note

            # process note 0
            call.process_note_0

            # increment current input note pointer
            exec.note_internal::increment_current_input_note_ptr

            # prepare note 1
            exec.note_internal::prepare_note

            # process note 1
            call.process_note_1

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        note_0_num_assets = notes.get_note(0).note().assets().num_assets(),
        note_1_num_assets = notes.get_note(1).note().assets().num_assets(),
        NOTE_0_ASSET_ASSERTIONS = construct_asset_assertions(notes.get_note(0).note()),
        NOTE_1_ASSET_ASSERTIONS = construct_asset_assertions(notes.get_note(1).note()),
    );

    tx_context.execute_code(&code).unwrap();
}

#[test]
fn test_get_inputs() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    fn construct_input_assertions(note: &Note) -> String {
        let mut code = String::new();
        for input_chunk in note.inputs().values().chunks(WORD_SIZE) {
            let mut input_word = EMPTY_WORD;
            input_word[..input_chunk.len()].copy_from_slice(input_chunk);

            code += &format!(
                "
                # assert the input is correct
                dup padw movup.4 mem_loadw push.{input_word} assert_eqw push.4 add
                ",
                input_word = word_to_masm_push_string(&input_word)
            );
        }
        code
    }

    let note0 = tx_context.input_notes().get_note(0).note();

    let code = format!(
        "
        use.kernel::prologue
        use.kernel::note->note_internal
        use.miden::note

        begin
            # => [BH, acct_id, IAH, NC]
            exec.prologue::prepare_transaction
            # => []

            exec.note_internal::prepare_note
            # => [NOTE_SCRIPT_ROOT, NOTE_ARGS]

            # drop the note inputs
            dropw dropw
            # => []

            push.{NOTE_0_PTR} exec.note::get_inputs
            # => [num_inputs, dest_ptr]

            eq.{num_inputs} assert
            # => [dest_ptr]

            dup eq.{NOTE_0_PTR} assert
            # => [dest_ptr]

            # apply note 1 input assertions
            {input_assertions}
            # => [dest_ptr]

            # clean the pointer
            drop
            # => []
        end
        ",
        num_inputs = note0.inputs().num_values(),
        input_assertions = construct_input_assertions(note0),
        NOTE_0_PTR = 100000000,
    );

    tx_context.execute_code(&code).unwrap();
}

/// This test checks the scenario when an input note has exactly 8 input values, and the transaction
/// script attempts to load the inputs to memory using the `miden::note::get_inputs` procedure.
///
/// Previously this setup was leading to the incorrect number of note input values computed during
/// the `get_inputs` procedure, see the
/// [issue #1363](https://github.com/0xMiden/miden-base/issues/1363) for more details.
#[test]
fn test_get_exactly_8_inputs() -> anyhow::Result<()> {
    let sender_id = ACCOUNT_ID_SENDER
        .try_into()
        .context("failed to convert ACCOUNT_ID_SENDER to account ID")?;
    let target_id = ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE.try_into().context(
        "failed to convert ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE to account ID",
    )?;

    // prepare note data
    let serial_num =
        RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]).draw_word();
    let tag = NoteTag::from_account_id(target_id);
    let metadata = NoteMetadata::new(
        sender_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .context("failed to create metadata")?;
    let vault = NoteAssets::new(vec![]).context("failed to create input note assets")?;
    let note_script = NoteScript::compile("begin nop end", TransactionKernel::assembler())
        .context("failed to compile note script")?;

    // create a recipient with note inputs, which number divides by 8. For simplicity create 8 input
    // values
    let recipient = NoteRecipient::new(
        serial_num,
        note_script,
        NoteInputs::new(vec![
            ONE,
            Felt::new(2),
            Felt::new(3),
            Felt::new(4),
            Felt::new(5),
            Felt::new(6),
            Felt::new(7),
            Felt::new(8),
        ])
        .context("failed to create note inputs")?,
    );
    let input_note = Note::new(vault.clone(), metadata, recipient);

    // provide this input note to the transaction context
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .input_notes(vec![input_note])
        .build();

    let tx_code = "
            use.kernel::prologue
            use.miden::note

            begin
                exec.prologue::prepare_transaction

                # execute the `get_inputs` procedure to trigger note inputs number assertion
                push.0 exec.note::get_inputs
                # => [num_inputs, 0]

                # assert that the number if inputs is 8
                push.8 assert_eq.err=\"number of inputs should be equal to 8\"

                # clean the stack
                drop
            end
        ";

    tx_context.execute_code(tx_code).context("transaction execution failed")?;

    Ok(())
}

#[test]
fn test_note_setup() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let code = "
        use.kernel::prologue
        use.kernel::note

        begin
            exec.prologue::prepare_transaction
            exec.note::prepare_note
            padw movup.4 mem_loadw

            # truncate the stack
            swapdw dropw dropw
        end
        ";

    let process = tx_context.execute_code(code).unwrap();

    note_setup_stack_assertions(&process, &tx_context);
    note_setup_memory_assertions(&process);
}

#[test]
fn test_note_script_and_note_args() -> miette::Result<()> {
    let note_args = [
        [Felt::new(91), Felt::new(91), Felt::new(91), Felt::new(91)],
        [Felt::new(92), Felt::new(92), Felt::new(92), Felt::new(92)],
    ];

    let mut tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let code = "
        use.kernel::prologue
        use.kernel::memory
        use.kernel::note

        begin
            exec.prologue::prepare_transaction
            exec.memory::get_num_input_notes push.2 assert_eq
            exec.note::prepare_note drop
            exec.note::increment_current_input_note_ptr drop
            exec.note::prepare_note drop

            # truncate the stack
            swapdw dropw dropw
        end
        ";

    let note_args_map = BTreeMap::from([
        (tx_context.input_notes().get_note(0).note().id(), note_args[1]),
        (tx_context.input_notes().get_note(1).note().id(), note_args[0]),
    ]);

    let tx_args = TransactionArgs::new(
        None,
        Some(note_args_map),
        tx_context.tx_args().advice_inputs().clone().map,
        Vec::<AccountInputs>::new(),
    );

    tx_context.set_tx_args(tx_args);
    let process = tx_context.execute_code(code)?;

    assert_eq!(process.stack.get_word(0), note_args[0]);

    assert_eq!(process.stack.get_word(1), note_args[1]);

    Ok(())
}

fn note_setup_stack_assertions(process: &Process, inputs: &TransactionContext) {
    let mut expected_stack = [ZERO; 16];

    // replace the top four elements with the tx script root
    let mut note_script_root = *inputs.input_notes().get_note(0).note().script().root();
    note_script_root.reverse();
    expected_stack[..4].copy_from_slice(&note_script_root);

    // assert that the stack contains the note inputs at the end of execution
    assert_eq!(process.stack.trace_state(), expected_stack)
}

fn note_setup_memory_assertions(process: &Process) {
    // assert that the correct pointer is stored in bookkeeping memory
    assert_eq!(
        read_root_mem_word(&process.into(), CURRENT_INPUT_NOTE_PTR)[0],
        Felt::from(input_note_data_ptr(0))
    );
}

#[test]
fn test_get_note_serial_number() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    // calling get_serial_number should return the serial number of the note
    let code = "
        use.kernel::prologue
        use.miden::note

        begin
            exec.prologue::prepare_transaction
            exec.note::get_serial_number

            # truncate the stack
            swapw dropw
        end
        ";

    let process = tx_context.execute_code(code).unwrap();

    let serial_number = tx_context.input_notes().get_note(0).note().serial_num();
    assert_eq!(process.stack.get_word(0), serial_number);
}

#[test]
fn test_get_inputs_hash() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let code = "
        use.std::sys

        use.miden::note

        begin
            # put the values that will be hashed into the memory
            push.1.2.3.4.4000 mem_storew dropw
            push.5.6.7.8.4004 mem_storew dropw
            push.9.10.11.12.4008 mem_storew dropw
            push.13.14.15.16.4012 mem_storew dropw

            # push the number of values and pointer to the inputs on the stack
            push.5.4000
            # execute the `compute_inputs_commitment` procedure for 5 values
            exec.note::compute_inputs_commitment
            # => [HASH_5]

            push.8.4000
            # execute the `compute_inputs_commitment` procedure for 8 values
            exec.note::compute_inputs_commitment
            # => [HASH_8, HASH_5]

            push.15.4000
            # execute the `compute_inputs_commitment` procedure for 15 values
            exec.note::compute_inputs_commitment
            # => [HASH_15, HASH_8, HASH_5]

            push.0.4000
            # check that calling `compute_inputs_commitment` procedure with 0 elements will result in an
            # empty word
            exec.note::compute_inputs_commitment
            # => [0, 0, 0, 0, HASH_15, HASH_8, HASH_5]

            # truncate the stack
            exec.sys::truncate_stack
        end
    ";

    let process = &tx_context.execute_code(code).unwrap();
    let process_state: ProcessState = process.into();

    let note_inputs_5_hash =
        NoteInputs::new(vec![Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4), Felt::new(5)])
            .unwrap()
            .commitment();

    let note_inputs_8_hash = NoteInputs::new(vec![
        Felt::new(1),
        Felt::new(2),
        Felt::new(3),
        Felt::new(4),
        Felt::new(5),
        Felt::new(6),
        Felt::new(7),
        Felt::new(8),
    ])
    .unwrap()
    .commitment();

    let note_inputs_15_hash = NoteInputs::new(vec![
        Felt::new(1),
        Felt::new(2),
        Felt::new(3),
        Felt::new(4),
        Felt::new(5),
        Felt::new(6),
        Felt::new(7),
        Felt::new(8),
        Felt::new(9),
        Felt::new(10),
        Felt::new(11),
        Felt::new(12),
        Felt::new(13),
        Felt::new(14),
        Felt::new(15),
    ])
    .unwrap()
    .commitment();

    let mut expected_stack = alloc::vec::Vec::new();

    expected_stack.extend_from_slice(note_inputs_5_hash.as_elements());
    expected_stack.extend_from_slice(note_inputs_8_hash.as_elements());
    expected_stack.extend_from_slice(note_inputs_15_hash.as_elements());
    expected_stack.extend_from_slice(&[ZERO, ZERO, ZERO, ZERO]);
    expected_stack.reverse();

    assert_eq!(process_state.get_stack_state()[0..16], expected_stack);
}

#[test]
fn test_get_current_script_root() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    // calling get_script_root should return script root
    let code = "
    use.kernel::prologue
    use.miden::note

    begin
        exec.prologue::prepare_transaction
        exec.note::get_script_root

        # truncate the stack
        swapw dropw
    end
    ";

    let process = tx_context.execute_code(code).unwrap();

    let script_root = tx_context.input_notes().get_note(0).note().script().root();
    assert_eq!(process.stack.get_word(0), script_root.as_elements());
}

#[test]
fn test_build_note_metadata() -> miette::Result<()> {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();
    let sender = tx_context.account().id();
    let receiver = AccountId::try_from(ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE).unwrap();

    let test_metadata1 = NoteMetadata::new(
        sender,
        NoteType::Private,
        NoteTag::from_account_id(receiver),
        NoteExecutionHint::after_block(500.into()).unwrap(),
        Felt::try_from(1u64 << 63).unwrap(),
    )
    .unwrap();
    let test_metadata2 = NoteMetadata::new(
        sender,
        NoteType::Public,
        // Use largest allowed use_case_id.
        NoteTag::for_public_use_case((1 << 14) - 1, u16::MAX, NoteExecutionMode::Local).unwrap(),
        NoteExecutionHint::on_block_slot(u8::MAX, u8::MAX, u8::MAX),
        Felt::try_from(0u64).unwrap(),
    )
    .unwrap();

    for (iteration, test_metadata) in [test_metadata1, test_metadata2].into_iter().enumerate() {
        let code = format!(
            "
        use.kernel::prologue
        use.kernel::tx

        begin
          exec.prologue::prepare_transaction
          push.{execution_hint}.{note_type}.{aux}.{tag}
          exec.tx::build_note_metadata

          # truncate the stack
          swapw dropw
        end
        ",
            execution_hint = Felt::from(test_metadata.execution_hint()),
            note_type = Felt::from(test_metadata.note_type()),
            aux = test_metadata.aux(),
            tag = test_metadata.tag(),
        );

        let process = tx_context.execute_code(&code)?;

        let metadata_word = [
            process.stack.get(3),
            process.stack.get(2),
            process.stack.get(1),
            process.stack.get(0),
        ];

        assert_eq!(Word::from(test_metadata), metadata_word, "failed in iteration {iteration}");
    }

    Ok(())
}

/// This serves as a test that setting a custom timestamp on mock chain blocks works.
#[test]
pub fn test_timelock() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    let account = mock_chain.add_pending_existing_wallet(Auth::Mock, vec![]);
    const TIMESTAMP_ERROR: MasmError = MasmError::from_static_str("123");

    let code = format!(
        r#"
      use.miden::note
      use.miden::tx

      begin
          # store the note inputs to memory starting at address 0
          push.0 exec.note::get_inputs
          # => [num_inputs, inputs_ptr]

          # make sure the number of inputs is 1
          eq.1 assert.err="number of note inputs is not 1"
          # => [inputs_ptr]

          # read the timestamp at which the note can be consumed
          mem_load
          # => [timestamp]

          exec.tx::get_block_timestamp
          # => [block_timestamp, timestamp]
          # ensure block timestamp is newer than timestamp

          lte assert.err="{}"
          # => []
      end"#,
        TIMESTAMP_ERROR.message()
    );

    let lock_timestamp = 2_000_000_000;
    let timelock_note = NoteBuilder::new(account.id(), &mut ChaCha20Rng::from_os_rng())
        .note_inputs([Felt::from(lock_timestamp)])
        .unwrap()
        .code(code.clone())
        .build(&TransactionKernel::testing_assembler_with_mock_account())
        .unwrap();

    mock_chain.add_pending_note(OutputNote::Full(timelock_note.clone()));
    mock_chain
        .prove_next_block_at(lock_timestamp - 100)
        .context("failed to prove next block at lock timestamp - 100")?;

    // Attempt to consume note too early.
    // ----------------------------------------------------------------------------------------
    let tx_inputs =
        mock_chain.get_transaction_inputs(account.clone(), None, &[timelock_note.id()], &[]);
    let tx_context = TransactionContextBuilder::new(account.clone())
        .tx_inputs(tx_inputs.clone())
        .build();
    let err = tx_context.execute().unwrap_err();
    let TransactionExecutorError::TransactionProgramExecutionFailed(err) = err else {
        panic!("unexpected error")
    };
    assert_execution_error!(Err::<(), _>(err), TIMESTAMP_ERROR);

    // Consume note where lock timestamp matches the block timestamp.
    // ----------------------------------------------------------------------------------------
    mock_chain
        .prove_next_block_at(lock_timestamp)
        .context("failed to prove next block at lock timestamp")?;

    let tx_inputs =
        mock_chain.get_transaction_inputs(account.clone(), None, &[timelock_note.id()], &[]);
    let tx_context = TransactionContextBuilder::new(account).tx_inputs(tx_inputs).build();
    tx_context.execute().unwrap();

    Ok(())
}

/// This test checks the scenario when some public key, which is provided to the RPO component of
/// the target account, is also provided as an input to the input note.
///
/// Previously this setup was leading to the values collision in the advice map, see the
/// [issue #1267](https://github.com/0xMiden/miden-base/issues/1267) for more details.
#[test]
fn test_public_key_as_note_input() {
    // this value will be used both as public key in the RPO component of the target account and as
    // well as the input of the input note
    let public_key_value: Word = [ZERO, ONE, Felt::new(2), Felt::new(3)];

    let mock_public_key = PublicKey::new(public_key_value);
    let rpo_component: AccountComponent = RpoFalcon512::new(mock_public_key).into();

    let mock_seed_1 = Digest::from([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]).as_bytes();
    let target_account = AccountBuilder::new(mock_seed_1)
        .with_auth_component(rpo_component.clone())
        .with_component(BasicWallet)
        .build_existing()
        .unwrap();

    let mock_seed_2 =
        Digest::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]).as_bytes();
    let sender_account = AccountBuilder::new(mock_seed_2)
        .with_auth_component(rpo_component)
        .with_component(BasicWallet)
        .build_existing()
        .unwrap();

    let serial_num =
        RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]).draw_word();
    let tag = NoteTag::from_account_id(target_account.id());
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .unwrap();
    let vault = NoteAssets::new(vec![]).unwrap();
    let note_script =
        NoteScript::compile("begin nop end", TransactionKernel::testing_assembler()).unwrap();
    let recipient = NoteRecipient::new(
        serial_num,
        note_script,
        NoteInputs::new(public_key_value.to_vec()).unwrap(),
    );
    let note_with_pub_key = Note::new(vault.clone(), metadata, recipient);

    let tx_context = TransactionContextBuilder::new(target_account)
        .input_notes(vec![note_with_pub_key])
        .build();
    tx_context.execute().unwrap();
}
