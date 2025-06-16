use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use assembly::Assembler;
use rand::Rng;

use crate::{
    Felt, NoteError, Word, ZERO,
    account::AccountId,
    asset::Asset,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient, NoteScript,
        NoteTag, NoteType,
    },
};

pub const DEFAULT_NOTE_CODE: &str = "begin nop end";

// NOTE BUILDER
// ================================================================================================

#[derive(Debug, Clone)]
pub struct NoteBuilder {
    sender: AccountId,
    inputs: Vec<Felt>,
    assets: Vec<Asset>,
    note_type: NoteType,
    note_execution_hint: NoteExecutionHint,
    serial_num: Word,
    tag: NoteTag,
    code: String,
    aux: Felt,
}

impl NoteBuilder {
    pub fn new<T: Rng>(sender: AccountId, mut rng: T) -> Self {
        let serial_num = [
            Felt::new(rng.random()),
            Felt::new(rng.random()),
            Felt::new(rng.random()),
            Felt::new(rng.random()),
        ];

        Self {
            sender,
            inputs: vec![],
            assets: vec![],
            note_type: NoteType::Public,
            note_execution_hint: NoteExecutionHint::None,
            serial_num,
            // The note tag is not under test, so we choose a value that is always valid.
            tag: NoteTag::from_account_id(sender),
            code: DEFAULT_NOTE_CODE.to_string(),
            aux: ZERO,
        }
    }

    /// Set the note's input to `inputs`.
    ///
    /// Note: This overwrite the inputs, the previous input values are discarded.
    pub fn note_inputs(
        mut self,
        inputs: impl IntoIterator<Item = Felt>,
    ) -> Result<Self, NoteError> {
        let validate = NoteInputs::new(inputs.into_iter().collect())?;
        self.inputs = validate.into();
        Ok(self)
    }

    pub fn add_assets(mut self, assets: impl IntoIterator<Item = Asset>) -> Self {
        self.assets.extend(assets);
        self
    }

    pub fn note_execution_hint(mut self, note_execution_hint: NoteExecutionHint) -> Self {
        self.note_execution_hint = note_execution_hint;
        self
    }

    pub fn tag(mut self, tag: u32) -> Self {
        self.tag = tag.into();
        self
    }

    pub fn note_type(mut self, note_type: NoteType) -> Self {
        self.note_type = note_type;
        self
    }

    pub fn code<S: AsRef<str>>(mut self, code: S) -> Self {
        self.code = code.as_ref().to_string();
        self
    }

    pub fn aux(mut self, aux: Felt) -> Self {
        self.aux = aux;
        self
    }

    pub fn build(self, assembler: &Assembler) -> Result<Note, NoteError> {
        let serial_num_part = self.serial_num[0].as_int();
        let source_manager = assembler.source_manager();
        let virtual_source_file =
            source_manager.load(&format!("note_{serial_num_part}"), self.code);
        let code = assembler
            .clone()
            .with_debug_mode(true)
            .assemble_program(virtual_source_file)
            .unwrap();

        let note_script = NoteScript::new(code);
        let vault = NoteAssets::new(self.assets)?;
        let metadata = NoteMetadata::new(
            self.sender,
            self.note_type,
            self.tag,
            self.note_execution_hint,
            self.aux,
        )?;
        let inputs = NoteInputs::new(self.inputs)?;
        let recipient = NoteRecipient::new(self.serial_num, note_script, inputs);

        Ok(Note::new(vault, metadata, recipient))
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
