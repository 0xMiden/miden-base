use alloc::{string::String, vec::Vec};

use miden_lib::{
    transaction::{
        TransactionKernel,
        memory::{
            NOTE_MEM_SIZE, NUM_OUTPUT_NOTES_PTR, OUTPUT_NOTE_ASSETS_OFFSET,
            OUTPUT_NOTE_METADATA_OFFSET, OUTPUT_NOTE_NUM_ASSETS_OFFSET,
            OUTPUT_NOTE_RECIPIENT_OFFSET, OUTPUT_NOTE_SECTION_OFFSET,
        },
    },
    utils::word_to_masm_push_string,
};
use miden_objects::{
    Felt, Hasher, ONE, Word, ZERO,
    asset::Asset,
    note::{Note, NoteExecutionHint, NoteType},
    testing::{account_id::ACCOUNT_ID_SENDER, note::NoteBuilder, storage::prepare_assets},
    vm::StackInputs,
};
use rand::rng;
use vm_processor::{ContextId, Process, ProcessState};

mod test_account;
mod test_account_delta;
mod test_asset;
mod test_asset_vault;
mod test_epilogue;
mod test_faucet;
mod test_fpi;
mod test_link_map;
mod test_note;
mod test_prologue;
mod test_tx;

// HELPER MACROS
// ================================================================================================

#[macro_export]
macro_rules! assert_execution_error {
    ($execution_result:expr, $expected_err:expr) => {
        match $execution_result {
            Err(vm_processor::ExecutionError::FailedAssertion { label: _, source_file: _, clk: _, err_code, err_msg }) => {
                if let Some(ref msg) = err_msg {
                  assert_eq!(msg.as_ref(), $expected_err.message(), "error messages did not match");
                }

                assert_eq!(
                    err_code, $expected_err.code(),
                    "Execution failed on assertion with an unexpected error (Actual code: {}, msg: {}, Expected code: {}).",
                    err_code, err_msg.as_ref().map(|string| string.as_ref()).unwrap_or("<no message>"), $expected_err,
                );
            },
            Ok(_) => panic!("Execution was unexpectedly successful"),
            Err(err) => panic!("Execution error was not as expected: {err}"),
        }
    };
}

// HELPER FUNCTIONS
// ================================================================================================

pub fn read_root_mem_word(process: &ProcessState, addr: u32) -> Word {
    process.get_mem_word(ContextId::root(), addr).unwrap().unwrap()
}

pub fn try_read_root_mem_word(process: &ProcessState, addr: u32) -> Option<Word> {
    process.get_mem_word(ContextId::root(), addr).unwrap()
}

/// Returns MASM code that defines a procedure called `create_mock_notes` which creates the notes
/// specified in `notes`, which stores output note metadata in the transaction host's memory.
pub fn create_mock_notes_procedure(notes: &[Note]) -> String {
    if notes.is_empty() {
        return String::new();
    }

    let mut script = String::from(
        "proc.create_mock_notes
            # remove padding from prologue
            dropw dropw dropw dropw
        ",
    );

    for (idx, note) in notes.iter().enumerate() {
        let metadata = word_to_masm_push_string(&note.metadata().into());
        let recipient = word_to_masm_push_string(&note.recipient().digest());
        let assets = prepare_assets(note.assets());
        let num_assets = assets.len();
        let note_offset = (idx as u32) * NOTE_MEM_SIZE;

        assert!(num_assets == 1, "notes are expected to have one asset only");

        script.push_str(&format!(
            "
                # populate note {idx}
                push.{metadata}
                push.{OUTPUT_NOTE_SECTION_OFFSET}.{note_offset}.{OUTPUT_NOTE_METADATA_OFFSET} add add mem_storew dropw
    
                push.{recipient}
                push.{OUTPUT_NOTE_SECTION_OFFSET}.{note_offset}.{OUTPUT_NOTE_RECIPIENT_OFFSET} add add mem_storew dropw
    
                push.{num_assets}
                push.{OUTPUT_NOTE_SECTION_OFFSET}.{note_offset}.{OUTPUT_NOTE_NUM_ASSETS_OFFSET} add add mem_store
    
                push.{first_asset}
                push.{OUTPUT_NOTE_SECTION_OFFSET}.{note_offset}.{OUTPUT_NOTE_ASSETS_OFFSET} add add mem_storew dropw
                ",
            idx = idx,
            metadata = metadata,
            recipient = recipient,
            num_assets = num_assets,
            first_asset = assets[0],
            note_offset = note_offset,
        ));
    }
    script.push_str(&format!(
        "# set num output notes
                push.{count}.{NUM_OUTPUT_NOTES_PTR} mem_store
            end
            ",
        count = notes.len(),
    ));

    script
}

/// Creates a note with a note script that creates all `notes` that get passed as a parameter.
///
/// `note_asset` is the asset that the note itself will contain
fn create_spawner_note(output_notes: Vec<&Note>) -> anyhow::Result<Note> {
    create_spawner_note_with_assets(output_notes, vec![])
}

/// Creates a note with a note script that creates all `notes` that get passed as a parameter,
/// and that carries the passed `asset`.
///
/// `assets` are the assets that the note itself will contain
fn create_spawner_note_with_assets(
    output_notes: Vec<&Note>,
    assets: Vec<Asset>,
) -> anyhow::Result<Note> {
    let note_code = note_script_that_creates_notes(output_notes);

    let note = NoteBuilder::new(ACCOUNT_ID_SENDER.try_into()?, rng())
        .code(note_code)
        .add_assets(assets)
        .build(&TransactionKernel::testing_assembler_with_mock_account())
        .unwrap();

    Ok(note)
}

/// Returns the code for a note that creates all notes in `output_notes`
fn note_script_that_creates_notes(output_notes: Vec<&Note>) -> String {
    let mut out =
        String::from("use.miden::contracts::wallets::basic->wallet\nuse.test::account\n\nbegin\n");

    for (idx, note) in output_notes.iter().enumerate() {
        if idx == 0 {
            out.push_str("padw padw\n");
        } else {
            out.push_str("dropw dropw dropw\n");
        }
        assert!(note.assets().iter().count() == 1, "output note is expected to have 1 asset");
        out.push_str(&format!(
            " push.{recipient}
              push.{hint}
              push.{note_type}
              push.{aux}
              push.{tag}
              call.wallet::create_note
              push.{asset}
              call.account::add_asset_to_note\n",
            recipient = word_to_masm_push_string(&note.recipient().digest()),
            hint = Felt::from(NoteExecutionHint::always()),
            note_type = NoteType::Public as u8,
            aux = note.metadata().aux(),
            tag = note.metadata().tag(),
            asset = prepare_assets(note.assets())[0],
        ));
    }

    out.push_str("repeat.5 dropw end\nend");
    out
}
