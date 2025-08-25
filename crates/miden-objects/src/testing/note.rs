use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_assembly::debuginfo::{SourceLanguage, SourceManagerSync, Uri};
use miden_assembly::{Assembler, DefaultSourceManager};
use rand::Rng;

use crate::account::AccountId;
use crate::asset::{Asset, FungibleAsset};
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
use crate::{Felt, NoteError, Word, ZERO};

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
    source_manager: Arc<dyn SourceManagerSync>,
}

impl NoteBuilder {
    pub fn new<T: Rng>(sender: AccountId, mut rng: T) -> Self {
        let serial_num = Word::from([
            Felt::new(rng.random()),
            Felt::new(rng.random()),
            Felt::new(rng.random()),
            Felt::new(rng.random()),
        ]);

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
            source_manager: Arc::new(DefaultSourceManager::default()),
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

    /// Note: this must be the same `source_manager` as used with `Assembler`.
    pub fn source_manager(mut self, source_manager: Arc<dyn SourceManagerSync>) -> Self {
        self.source_manager = source_manager;
        self
    }

    pub fn build(self, assembler: &Assembler) -> Result<Note, NoteError> {
        // Generate a unique file name from the note's serial number, which should be unique per
        // note. Only includes two elements in the file name which should be enough for the
        // uniqueness in the testing context and does not result in overly long file names which do
        // not render well in all situations.
        let virtual_source_file = self.source_manager.load(
            SourceLanguage::Masm,
            Uri::new(format!(
                "note_{:x}{:x}",
                self.serial_num[0].as_int(),
                self.serial_num[1].as_int()
            )),
            self.code,
        );
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
