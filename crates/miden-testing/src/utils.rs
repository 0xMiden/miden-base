use alloc::string::String;

use miden_lib::transaction::{TransactionKernel, memory};
use miden_objects::{account::AccountId, asset::Asset, note::Note, testing::note::NoteBuilder};
use rand::{SeedableRng, rngs::SmallRng};

// TEST HELPERS
// ================================================================================================

pub fn input_note_data_ptr(note_idx: u32) -> memory::MemoryAddress {
    memory::INPUT_NOTE_DATA_SECTION_OFFSET + note_idx * memory::NOTE_MEM_SIZE
}

/// Creates a note that carries `assets` and a script that moves the assets into the account's
/// vault.
///
/// The created note does not require account authentication and can be consumed by any account.
pub fn create_transfer_mock_note(sender: AccountId, assets: &[Asset]) -> Note {
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
        use.miden::note
        use.miden::contracts::wallets::basic->wallet

        begin
            # fetch pointer & number of assets
            push.0 exec.note::get_assets          # [num_assets, dest_ptr]

            # runtime-check we got the expected count
            push.{num_assets} assert_eq             # [dest_ptr]

            {code_body}
            push.1 call.::miden::account::incr_nonce drop
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
