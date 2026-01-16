use alloc::collections::BTreeMap;
use alloc::string::String;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;

use super::type_registry::SchemaRequirement;
use super::{InitStorageData, StorageValueName, WordSchema};
use crate::Word;
use crate::account::StorageSlotName;
use crate::errors::AccountComponentTemplateError;

// VALUE SLOT SCHEMA
// ================================================================================================

/// Describes the schema for a storage value slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSlotSchema {
    description: Option<String>,
    word: WordSchema,
}

impl ValueSlotSchema {
    pub fn new(description: Option<String>, word: WordSchema) -> Self {
        Self { description, word }
    }

    pub fn description(&self) -> Option<&String> {
        self.description.as_ref()
    }

    pub fn word(&self) -> &WordSchema {
        &self.word
    }

    pub(super) fn collect_init_value_requirements(
        &self,
        value_name: StorageValueName,
        requirements: &mut BTreeMap<StorageValueName, SchemaRequirement>,
    ) -> Result<(), AccountComponentTemplateError> {
        self.word.collect_init_value_requirements(
            value_name,
            self.description.clone(),
            requirements,
        )
    }

    /// Builds a [Word] from the provided initialization data using the inner word schema.
    pub fn try_build_word(
        &self,
        init_storage_data: &InitStorageData,
        slot_name: &StorageSlotName,
    ) -> Result<Word, AccountComponentTemplateError> {
        self.word.try_build_word(init_storage_data, slot_name)
    }

    pub(crate) fn validate(
        &self,
        _slot_name: &StorageSlotName,
    ) -> Result<(), AccountComponentTemplateError> {
        self.word.validate()?;
        Ok(())
    }
}

impl Serializable for ValueSlotSchema {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(&self.description);
        target.write(&self.word);
    }
}

impl Deserializable for ValueSlotSchema {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let description = Option::<String>::read_from(source)?;
        let word = WordSchema::read_from(source)?;
        Ok(ValueSlotSchema::new(description, word))
    }
}
