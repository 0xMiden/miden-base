use alloc::{string::String, vec::Vec};

use miden_lib::transaction::{TransactionKernel, memory};
use miden_objects::{
    account::AccountId,
    asset::Asset,
    note::Note,
    testing::{note::NoteBuilder, storage::prepare_assets},
};
use miden_tx::utils::word_to_masm_push_string;
use rand::{SeedableRng, rngs::SmallRng};
use vm_processor::Felt;

// TEST HELPERS
// ================================================================================================

pub fn input_note_data_ptr(note_idx: u32) -> memory::MemoryAddress {
    memory::INPUT_NOTE_DATA_SECTION_OFFSET + note_idx * memory::NOTE_MEM_SIZE
}

// HELPER NOTES
// ================================================================================================

/// Creates a `P2ANY` note.
///
/// A `P2ANY` note carries `assets` and a script that moves the assets into the executing account's
/// vault.
///
/// The created note does not require authentication and can be consumed by any account.
pub fn create_p2any_note(sender: AccountId, assets: &[Asset]) -> Note {
    assert!(!assets.is_empty(), "note must carry at least one asset");

    let mut code_body = String::new();
    for i in 0..assets.len() {
        if i == 0 {
            // first asset (dest_ptr is already on stack)
            code_body.push_str(
                "
                # add first asset
                
                padw dup.4 mem_loadw
                padw swapw padw padw swapdw
                call.wallet::receive_asset      
                dropw movup.12
                # => [dest_ptr, pad(12)]
                ",
            );
        } else {
            code_body.push_str(
                "
                # add next asset

                add.4 dup movdn.13
                padw movup.4 mem_loadw
                call.wallet::receive_asset
                dropw movup.12
                # => [dest_ptr, pad(12)]",
            );
        }
    }
    code_body.push_str("dropw dropw dropw dropw");

    let code = format!(
        "
        use.test::account
        use.miden::note
        use.miden::contracts::wallets::basic->wallet

        begin
            # fetch pointer & number of assets
            push.0 exec.note::get_assets          # [num_assets, dest_ptr]

            # runtime-check we got the expected count
            push.{num_assets} assert_eq             # [dest_ptr]

            {code_body}
            dropw dropw dropw dropw
        end
        ",
        num_assets = assets.len(),
    );

    NoteBuilder::new(sender, SmallRng::from_seed([0; 32]))
        .add_assets(assets.iter().copied())
        .code(code)
        .build(&TransactionKernel::testing_assembler_with_mock_account())
        .expect("generated note script should compile")
}

/// Creates a `SPAWN` note.
///
///  A `SPAWN` note contains a note script that creates all `output_notes` that get passed as a
///  parameter.
pub fn create_spawn_note(sender_id: AccountId, output_notes: Vec<&Note>) -> anyhow::Result<Note> {
    let note_code = note_script_that_creates_notes(output_notes);

    let note = NoteBuilder::new(sender_id, SmallRng::from_os_rng())
        .code(note_code)
        .build(&TransactionKernel::testing_assembler_with_mock_account())?;

    Ok(note)
}

/// Returns the code for a note that creates all notes in `output_notes`
fn note_script_that_creates_notes(output_notes: Vec<&Note>) -> String {
    let mut out =
        String::from("use.miden::tx\nuse.test::account\n\nbegin\n");

    for (idx, note) in output_notes.iter().enumerate() {
        if idx == 0 {
            out.push_str("padw padw\n");
        } else {
            out.push_str("dropw dropw dropw\n");
        }
        let assets_str = prepare_assets(note.assets());
        out.push_str(&format!(
            " push.{recipient}
              push.{hint}
              push.{note_type}
              push.{aux}
              push.{tag}
              call.tx::create_note\n",
            recipient = word_to_masm_push_string(&note.recipient().digest()),
            hint = Felt::from(note.metadata().execution_hint()),
            note_type = note.metadata().note_type() as u8,
            aux = note.metadata().aux(),
            tag = note.metadata().tag(),
        ));

        for asset in assets_str {
            out.push_str(&format!(
                " push.{asset}
                  call.account::add_asset_to_note\n",
            ));
        }
    }

    out.push_str("repeat.5 dropw end\nend");
    out
}
