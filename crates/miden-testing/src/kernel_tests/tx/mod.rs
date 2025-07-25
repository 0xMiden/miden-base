use alloc::string::String;

use miden_lib::{
    transaction::memory::{
        NOTE_MEM_SIZE, NUM_OUTPUT_NOTES_PTR, OUTPUT_NOTE_ASSETS_OFFSET,
        OUTPUT_NOTE_DIRTY_FLAG_OFFSET, OUTPUT_NOTE_METADATA_OFFSET, OUTPUT_NOTE_NUM_ASSETS_OFFSET,
        OUTPUT_NOTE_RECIPIENT_OFFSET, OUTPUT_NOTE_SECTION_OFFSET,
    },
    utils::word_to_masm_push_string,
};
use miden_objects::{
    Felt, Hasher, ONE, Word, ZERO, note::Note, testing::storage::prepare_assets, vm::StackInputs,
};
use vm_processor::{ContextId, Process};

mod test_account;
mod test_account_delta;
mod test_account_interface;
mod test_asset;
mod test_asset_vault;
mod test_auth;
mod test_epilogue;
mod test_faucet;
mod test_fpi;
mod test_input_note;
mod test_link_map;
mod test_note;
mod test_output_note;
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

/// Extension trait for a [`Process`] to conveniently read kernel memory.
pub trait ProcessMemoryExt {
    /// Reads a word from transaction kernel memory.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the address is not word-aligned.
    /// - the memory location is not initialized.
    fn get_kernel_mem_word(&self, addr: u32) -> Word;

    /// Reads a word from transaction kernel memory.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the address is not word-aligned.
    fn try_get_kernel_mem_word(&self, addr: u32) -> Option<Word>;
}

impl ProcessMemoryExt for Process {
    fn get_kernel_mem_word(&self, addr: u32) -> Word {
        self.try_get_kernel_mem_word(addr).expect("expected address to be initialized")
    }

    fn try_get_kernel_mem_word(&self, addr: u32) -> Option<Word> {
        self.chiplets
            .memory
            .get_word(ContextId::root(), addr)
            .expect("expected address to be word-aligned")
    }
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

                push.1 # dirty flag should be `1` by default
                push.{OUTPUT_NOTE_SECTION_OFFSET}.{note_offset}.{OUTPUT_NOTE_DIRTY_FLAG_OFFSET} add add mem_store
    
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
