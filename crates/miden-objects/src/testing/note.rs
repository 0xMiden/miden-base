use alloc::vec::Vec;

use crate::assembly::Assembler;
use crate::asset::FungibleAsset;
use crate::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    NoteTag,
    NoteType,
};
use crate::testing::account_id::ACCOUNT_ID_SENDER;
use crate::{Word, ZERO};

pub const DEFAULT_NOTE_CODE: &str = "begin nop end";

impl Note {
    /// Returns a note with no-op code and one asset.
    pub fn mock_noop(serial_num: Word) -> Note {
        let sender_id = ACCOUNT_ID_SENDER.try_into().unwrap();
        let note_script = NoteScript::mock();
        let assets =
            NoteAssets::new(vec![FungibleAsset::mock(200)]).expect("note assets should be valid");
        let metadata = NoteMetadata::new(
            sender_id,
            NoteType::Private,
            NoteTag::from_account_id(sender_id),
            NoteExecutionHint::Always,
            ZERO,
        )
        .unwrap();
        let inputs = NoteInputs::new(Vec::new()).unwrap();
        let recipient = NoteRecipient::new(serial_num, note_script, inputs);

        Note::new(assets, metadata, recipient)
    }
}

// NOTE SCRIPT
// ================================================================================================

impl NoteScript {
    pub fn mock() -> Self {
        let assembler = Assembler::default();
        let code = assembler.assemble_program(DEFAULT_NOTE_CODE).unwrap();
        Self::new(code)
    }
}
