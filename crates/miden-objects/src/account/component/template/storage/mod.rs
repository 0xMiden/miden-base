use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use vm_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use vm_core::{Felt, FieldElement};
use vm_processor::DeserializationError;

mod entry_content;
pub use entry_content::*;

use super::AccountComponentTemplateError;
use crate::Word;
use crate::account::StorageSlot;

mod placeholder;
pub use placeholder::{
    PlaceholderTypeRequirement,
    StorageValueName,
    StorageValueNameError,
    TemplateType,
    TemplateTypeError,
};

mod init_storage_data;
pub use init_storage_data::InitStorageData;

#[cfg(feature = "std")]
pub mod toml;

/// Alias used for iterators that collect all placeholders and their types within a component
/// template.
pub type TemplateRequirementsIter<'a> =
    Box<dyn Iterator<Item = (StorageValueName, PlaceholderTypeRequirement)> + 'a>;

// IDENTIFIER
// ================================================================================================

/// An identifier for a storage entry field.
///
/// An identifier consists of a name that identifies the field, and an optional description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIdentifier {
    /// A human-readable identifier for the template.
    pub name: StorageValueName,
    /// An optional description explaining the purpose of this template.
    pub description: Option<String>,
}

impl FieldIdentifier {
    /// Creates a new `FieldIdentifier` with the given name and no description.
    pub fn with_name(name: StorageValueName) -> Self {
        Self { name, description: None }
    }

    /// Creates a new `FieldIdentifier` with the given name and description.
    pub fn with_description(name: StorageValueName, description: impl Into<String>) -> Self {
        Self {
            name,
            description: Some(description.into()),
        }
    }

    /// Returns the identifier name.
    pub fn name(&self) -> &StorageValueName {
        &self.name
    }

    /// Returns the identifier description.
    pub fn description(&self) -> Option<&String> {
        self.description.as_ref()
    }
}

impl From<StorageValueName> for FieldIdentifier {
    fn from(value: StorageValueName) -> Self {
        FieldIdentifier::with_name(value)
    }
}

impl Serializable for FieldIdentifier {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(&self.name);
        target.write(&self.description);
    }
}

impl Deserializable for FieldIdentifier {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let name = StorageValueName::read_from(source)?;
        let description = Option::<String>::read_from(source)?;
        Ok(FieldIdentifier { name, description })
    }
}

// STORAGE ENTRY
// ================================================================================================

/// Represents a single entry in the component's storage layout.
///
/// Each entry can describe:
/// - A value slot with a single word.
/// - A map slot with a key-value map that occupies one storage slot.
/// - A multi-slot entry spanning multiple contiguous slots with multiple words (but not maps) that
///   represent a single logical value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum StorageEntry {
    /// A value slot, which can contain one word.
    Value {
        /// The numeric index of this map slot in the component's storage.
        slot: u8,
        /// A description of a word, representing either a predefined value or a templated one.
        word_entry: WordRepresentation,
    },

    /// A map slot, containing multiple key-value pairs. Keys and values are hex-encoded strings.
    Map {
        /// The numeric index of this map slot in the component's storage.
        slot: u8,
        /// A list of key-value pairs to initialize in this map slot.
        map: MapRepresentation,
    },

    /// A multi-slot entry, representing a single logical value across multiple slots.
    MultiSlot {
        /// The indices of the slots that form this multi-slot entry.
        slots: Range<u8>,
        /// A description of the values.
        word_entries: MultiWordRepresentation,
    },
}

impl StorageEntry {
    pub fn new_value(slot: u8, word_entry: impl Into<WordRepresentation>) -> Self {
        StorageEntry::Value { slot, word_entry: word_entry.into() }
    }

    pub fn new_map(slot: u8, map: MapRepresentation) -> Self {
        StorageEntry::Map { slot, map }
    }

    pub fn new_multislot(
        identifier: FieldIdentifier,
        slots: Range<u8>,
        values: Vec<[FeltRepresentation; 4]>,
    ) -> Self {
        StorageEntry::MultiSlot {
            slots,
            word_entries: MultiWordRepresentation::Value { identifier, values },
        }
    }

    pub fn name(&self) -> Option<&StorageValueName> {
        match self {
            StorageEntry::Value { word_entry, .. } => word_entry.name(),
            StorageEntry::Map { map, .. } => Some(map.name()),
            StorageEntry::MultiSlot { word_entries, .. } => match word_entries {
                MultiWordRepresentation::Value { identifier, .. } => Some(&identifier.name),
            },
        }
    }

    /// Returns the slot indices that the storage entry covers.
    pub fn slot_indices(&self) -> Range<u8> {
        match self {
            StorageEntry::MultiSlot { slots, .. } => slots.clone(),
            StorageEntry::Value { slot, .. } | StorageEntry::Map { slot, .. } => *slot..*slot + 1,
        }
    }

    /// Returns an iterator over all of the storage entries's value names, alongside their
    /// expected type.
    pub fn template_requirements(&self) -> TemplateRequirementsIter<'_> {
        match self {
            StorageEntry::Value { word_entry, .. } => {
                word_entry.template_requirements(StorageValueName::empty())
            },
            StorageEntry::Map { map, .. } => map.template_requirements(),
            StorageEntry::MultiSlot { word_entries, .. } => match word_entries {
                MultiWordRepresentation::Value { identifier, values } => {
                    Box::new(values.iter().flat_map(move |word| {
                        word.iter()
                            .flat_map(move |f| f.template_requirements(identifier.name.clone()))
                    }))
                },
            },
        }
    }

    /// Attempts to convert the storage entry into a list of [`StorageSlot`].
    ///
    /// - [`StorageEntry::Value`] would convert to a [`StorageSlot::Value`]
    /// - [`StorageEntry::MultiSlot`] would convert to as many [`StorageSlot::Value`] as required by
    ///   the defined type
    /// - [`StorageEntry::Map`] would convert to a [`StorageSlot::Map`]
    ///
    /// Each of the entry's values could be templated. These values are replaced for values found
    /// in `init_storage_data`, identified by its key.
    pub fn try_build_storage_slots(
        &self,
        init_storage_data: &InitStorageData,
    ) -> Result<Vec<StorageSlot>, AccountComponentTemplateError> {
        match self {
            StorageEntry::Value { word_entry, .. } => {
                let slot =
                    word_entry.try_build_word(init_storage_data, StorageValueName::empty())?;
                Ok(vec![StorageSlot::Value(slot)])
            },
            StorageEntry::Map { map, .. } => {
                let storage_map = map.try_build_map(init_storage_data)?;
                Ok(vec![StorageSlot::Map(storage_map)])
            },
            StorageEntry::MultiSlot { word_entries, .. } => {
                match word_entries {
                    MultiWordRepresentation::Value { identifier, values } => {
                        Ok(values
                            .iter()
                            .map(|word_repr| {
                                let mut result = [Felt::ZERO; 4];

                                for (index, felt_repr) in word_repr.iter().enumerate() {
                                    result[index] = felt_repr.try_build_felt(
                                        init_storage_data,
                                        identifier.name.clone(),
                                    )?;
                                }
                                // SAFETY: result is guaranteed to have all its 4 indices rewritten
                                Ok(StorageSlot::Value(Word::from(result)))
                            })
                            .collect::<Result<Vec<StorageSlot>, _>>()?)
                    },
                }
            },
        }
    }

    /// Validates the storage entry for internal consistency.
    pub(super) fn validate(&self) -> Result<(), AccountComponentTemplateError> {
        match self {
            StorageEntry::Map { map, .. } => map.validate(),
            StorageEntry::MultiSlot { slots, word_entries, .. } => {
                if slots.len() == 1 {
                    return Err(AccountComponentTemplateError::MultiSlotSpansOneSlot);
                }

                if slots.len() != word_entries.num_words() {
                    return Err(AccountComponentTemplateError::MultiSlotArityMismatch);
                }

                word_entries.validate()
            },
            StorageEntry::Value { word_entry, .. } => Ok(word_entry.validate()?),
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for StorageEntry {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            StorageEntry::Value { slot, word_entry } => {
                target.write_u8(0u8);
                target.write_u8(*slot);
                target.write(word_entry);
            },
            StorageEntry::Map { slot, map } => {
                target.write_u8(1u8);
                target.write_u8(*slot);
                target.write(map);
            },
            StorageEntry::MultiSlot { word_entries, slots } => {
                target.write_u8(2u8);
                target.write(word_entries);
                target.write(slots.start);
                target.write(slots.end);
            },
        }
    }
}

impl Deserializable for StorageEntry {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let variant_tag = source.read_u8()?;
        match variant_tag {
            0 => {
                let slot = source.read_u8()?;
                let word_entry: WordRepresentation = source.read()?;
                Ok(StorageEntry::Value { slot, word_entry })
            },
            1 => {
                let slot = source.read_u8()?;
                let map: MapRepresentation = source.read()?;
                Ok(StorageEntry::Map { slot, map })
            },
            2 => {
                let word_entries: MultiWordRepresentation = source.read()?;
                let slots_start: u8 = source.read()?;
                let slots_end: u8 = source.read()?;
                Ok(StorageEntry::MultiSlot {
                    slots: slots_start..slots_end,
                    word_entries,
                })
            },
            _ => Err(DeserializationError::InvalidValue(format!(
                "unknown variant tag '{variant_tag}' for StorageEntry"
            ))),
        }
    }
}

// MAP ENTRY
// ================================================================================================

/// Key-value entry for storage maps.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct MapEntry {
    key: WordRepresentation,
    value: WordRepresentation,
}

impl MapEntry {
    pub fn new(key: impl Into<WordRepresentation>, value: impl Into<WordRepresentation>) -> Self {
        Self { key: key.into(), value: value.into() }
    }

    pub fn key(&self) -> &WordRepresentation {
        &self.key
    }

    pub fn value(&self) -> &WordRepresentation {
        &self.value
    }

    pub fn into_parts(self) -> (WordRepresentation, WordRepresentation) {
        let MapEntry { key, value } = self;
        (key, value)
    }

    pub fn template_requirements(
        &self,
        placeholder_prefix: StorageValueName,
    ) -> TemplateRequirementsIter<'_> {
        let key_iter = self.key.template_requirements(placeholder_prefix.clone());
        let value_iter = self.value.template_requirements(placeholder_prefix);

        Box::new(key_iter.chain(value_iter))
    }
}

impl Serializable for MapEntry {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.key.write_into(target);
        self.value.write_into(target);
    }
}

impl Deserializable for MapEntry {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let key = WordRepresentation::read_from(source)?;
        let value = WordRepresentation::read_from(source)?;
        Ok(MapEntry { key, value })
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;
    use alloc::string::ToString;
    use core::error::Error;
    use core::panic;

    use assembly::Assembler;
    use semver::Version;
    use vm_core::utils::{Deserializable, Serializable};
    use vm_core::{EMPTY_WORD, Felt, Word};

    use crate::account::component::FieldIdentifier;
    use crate::account::component::template::storage::placeholder::TemplateType;
    use crate::account::component::template::{
        AccountComponentMetadata,
        InitStorageData,
        MapEntry,
        MapRepresentation,
        StorageValueName,
    };
    use crate::account::{
        AccountComponent,
        AccountComponentTemplate,
        AccountType,
        FeltRepresentation,
        StorageEntry,
        StorageSlot,
        TemplateTypeError,
        WordRepresentation,
    };
    use crate::errors::AccountComponentTemplateError;
    use crate::testing::account_code::CODE;
    use crate::{AccountError, word};

    #[test]
    fn test_storage_entry_serialization() {
        let felt_array: [FeltRepresentation; 4] = [
            FeltRepresentation::from(Felt::new(0xabc)),
            FeltRepresentation::from(Felt::new(1218)),
            FeltRepresentation::from(Felt::new(0xdba3)),
            FeltRepresentation::new_template(
                TemplateType::native_felt(),
                StorageValueName::new("slot3").unwrap(),
            )
            .with_description("dummy description"),
        ];

        let test_word: Word = word!("0x000001");
        let test_word = test_word.map(FeltRepresentation::from);

        let map_representation = MapRepresentation::new(
            vec![
                MapEntry {
                    key: WordRepresentation::new_template(
                        TemplateType::native_word(),
                        StorageValueName::new("foo").unwrap().into(),
                    ),
                    value: WordRepresentation::new_value(test_word.clone(), None),
                },
                MapEntry {
                    key: WordRepresentation::new_value(test_word.clone(), None),
                    value: WordRepresentation::new_template(
                        TemplateType::native_word(),
                        StorageValueName::new("bar").unwrap().into(),
                    ),
                },
                MapEntry {
                    key: WordRepresentation::new_template(
                        TemplateType::native_word(),
                        StorageValueName::new("baz").unwrap().into(),
                    ),
                    value: WordRepresentation::new_value(test_word, None),
                },
            ],
            StorageValueName::new("map").unwrap(),
        )
        .with_description("a storage map description");

        let storage = vec![
            StorageEntry::new_value(0, felt_array.clone()),
            StorageEntry::new_map(1, map_representation),
            StorageEntry::new_multislot(
                FieldIdentifier::with_description(
                    StorageValueName::new("multi").unwrap(),
                    "Multi slot entry",
                ),
                2..4,
                vec![
                    [
                        FeltRepresentation::new_template(
                            TemplateType::native_felt(),
                            StorageValueName::new("test").unwrap(),
                        ),
                        FeltRepresentation::new_template(
                            TemplateType::native_felt(),
                            StorageValueName::new("test2").unwrap(),
                        ),
                        FeltRepresentation::new_template(
                            TemplateType::native_felt(),
                            StorageValueName::new("test3").unwrap(),
                        ),
                        FeltRepresentation::new_template(
                            TemplateType::native_felt(),
                            StorageValueName::new("test4").unwrap(),
                        ),
                    ],
                    felt_array,
                ],
            ),
            StorageEntry::new_value(
                4,
                WordRepresentation::new_template(
                    TemplateType::native_word(),
                    StorageValueName::new("single").unwrap().into(),
                ),
            ),
        ];

        let config = AccountComponentMetadata {
            name: "Test Component".into(),
            description: "This is a test component".into(),
            version: Version::parse("1.0.0").unwrap(),
            supported_types: BTreeSet::from([AccountType::FungibleFaucet]),
            storage,
        };
        let toml = config.as_toml().unwrap();
        let deserialized = AccountComponentMetadata::from_toml(&toml).unwrap();

        assert_eq!(deserialized, config);
    }

    #[test]
    pub fn toml_serde_roundtrip() {
        let toml_text = r#"
        name = "Test Component"
        description = "This is a test component"
        version = "1.0.1"
        supported-types = ["FungibleFaucet", "RegularAccountImmutableCode"]

        [[storage]]
        name = "map_entry"
        slot = 0
        values = [
            { key = "0x1", value = ["0x1","0x2","0x3","0"]},
            { key = "0x3", value = "0x123" }, 
            { key = { name = "map_key_template", description = "this tests that the default type is correctly set"}, value = "0x3" },
        ]

        [[storage]]
        name = "token_metadata"
        description = "Contains metadata about the token associated to the faucet account"
        slot = 1
        value = [
            { type = "felt", name = "max_supply", description = "Maximum supply of the token in base units" }, # placeholder
            { type = "token_symbol", value = "TST" }, # hardcoded non-felt type
            { type = "u8", name = "decimals", description = "Number of decimal places" }, # placeholder
            { value = "0" }, 
        ]

        [[storage]]
        name = "default_recallable_height"
        slot = 2
        type = "word"
        "#;

        let component_metadata = AccountComponentMetadata::from_toml(toml_text).unwrap();
        let requirements = component_metadata.get_placeholder_requirements();

        assert_eq!(requirements.len(), 4);

        let supply = requirements
            .get(&StorageValueName::new("token_metadata.max_supply").unwrap())
            .unwrap();
        assert_eq!(supply.r#type.as_str(), "felt");

        let decimals = requirements
            .get(&StorageValueName::new("token_metadata.decimals").unwrap())
            .unwrap();
        assert_eq!(decimals.r#type.as_str(), "u8");

        let default_recallable_height = requirements
            .get(&StorageValueName::new("default_recallable_height").unwrap())
            .unwrap();
        assert_eq!(default_recallable_height.r#type.as_str(), "word");

        let map_key_template = requirements
            .get(&StorageValueName::new("map_entry.map_key_template").unwrap())
            .unwrap();
        assert_eq!(map_key_template.r#type.as_str(), "word");

        let library = Assembler::default().assemble_library([CODE]).unwrap();
        let template = AccountComponentTemplate::new(component_metadata, library);

        let template_bytes = template.to_bytes();
        let template_deserialized =
            AccountComponentTemplate::read_from_bytes(&template_bytes).unwrap();
        assert_eq!(template, template_deserialized);

        // Fail to parse because 2800 > u8
        let storage_placeholders = InitStorageData::new([
            (
                StorageValueName::new("map_entry.map_key_template").unwrap(),
                "0x123".to_string(),
            ),
            (
                StorageValueName::new("token_metadata.max_supply").unwrap(),
                20_000u64.to_string(),
            ),
            (StorageValueName::new("token_metadata.decimals").unwrap(), "2800".into()),
            (StorageValueName::new("default_recallable_height").unwrap(), "0".into()),
        ]);

        let component = AccountComponent::from_template(&template, &storage_placeholders);
        assert_matches::assert_matches!(
            component,
            Err(AccountError::AccountComponentTemplateInstantiationError(
                AccountComponentTemplateError::StorageValueParsingError(
                    TemplateTypeError::ParseError { .. }
                )
            ))
        );

        // Instantiate successfully
        let storage_placeholders = InitStorageData::new([
            (
                StorageValueName::new("map_entry.map_key_template").unwrap(),
                "0x123".to_string(),
            ),
            (
                StorageValueName::new("token_metadata.max_supply").unwrap(),
                20_000u64.to_string(),
            ),
            (StorageValueName::new("token_metadata.decimals").unwrap(), "128".into()),
            (StorageValueName::new("default_recallable_height").unwrap(), "0x0".into()),
        ]);

        let component = AccountComponent::from_template(&template, &storage_placeholders).unwrap();
        assert_eq!(
            component.supported_types(),
            &[AccountType::FungibleFaucet, AccountType::RegularAccountImmutableCode]
                .into_iter()
                .collect()
        );

        let storage_map = component.storage_slots.first().unwrap();
        match storage_map {
            StorageSlot::Map(storage_map) => assert_eq!(storage_map.entries().count(), 3),
            _ => panic!("should be map"),
        }

        let value_entry = component.storage_slots().get(2).unwrap();
        match value_entry {
            StorageSlot::Value(v) => {
                assert_eq!(v, &EMPTY_WORD)
            },
            _ => panic!("should be value"),
        }

        let failed_instantiation =
            AccountComponent::from_template(&template, &InitStorageData::default());

        assert_matches::assert_matches!(
            failed_instantiation,
            Err(AccountError::AccountComponentTemplateInstantiationError(
                AccountComponentTemplateError::PlaceholderValueNotProvided(_)
            ))
        );
    }

    #[test]
    fn test_no_duplicate_slot_names() {
        let toml_text = r#"
        name = "Test Component"
        description = "This is a test component"
        version = "1.0.1"
        supported-types = ["FungibleFaucet", "RegularAccountImmutableCode"]

        [[storage]]
        name = "test_duplicate"
        slot = 0
        type = "felt" # Felt is not a valid type for word slots
        "#;

        let err = AccountComponentMetadata::from_toml(toml_text).unwrap_err();
        assert_matches::assert_matches!(err, AccountComponentTemplateError::InvalidType(_, _))
    }

    #[test]
    fn toml_fail_multislot_arity_mismatch() {
        let toml_text = r#"
        name = "Test Component"
        description = "Test multislot arity mismatch"
        version = "1.0.1"
        supported-types = ["FungibleFaucet"]

        [[storage]]
        name = "multislot_test"
        slots = [0, 1]
        values = [
            [ "0x1", "0x2", "0x3", "0x4" ]
        ]
    "#;

        let err = AccountComponentMetadata::from_toml(toml_text).unwrap_err();
        assert_matches::assert_matches!(err, AccountComponentTemplateError::MultiSlotArityMismatch);
    }

    #[test]
    fn toml_fail_multislot_duplicate_slot() {
        let toml_text = r#"
        name = "Test Component"
        description = "Test multislot duplicate slot"
        version = "1.0.1"
        supported-types = ["FungibleFaucet"]

        [[storage]]
        name = "multislot_duplicate"
        slots = [0, 1]
        values = [
            [ "0x1", "0x2", "0x3", "0x4" ],
            [ "0x5", "0x6", "0x7", "0x8" ]
        ]

        [[storage]]
        name = "multislot_duplicate"
        slots = [1, 2]
        values = [
            [ "0x1", "0x2", "0x3", "0x4" ],
            [ "0x5", "0x6", "0x7", "0x8" ]
        ]
    "#;

        let err = AccountComponentMetadata::from_toml(toml_text).unwrap_err();
        assert_matches::assert_matches!(err, AccountComponentTemplateError::DuplicateSlot(1));
    }

    #[test]
    fn toml_fail_multislot_non_contiguous_slots() {
        let toml_text = r#"
        name = "Test Component"
        description = "Test multislot non contiguous"
        version = "1.0.1"
        supported-types = ["FungibleFaucet"]

        [[storage]]
        name = "multislot_non_contiguous"
        slots = [0, 2]
        values = [
            [ "0x1", "0x2", "0x3", "0x4" ],
            [ "0x5", "0x6", "0x7", "0x8" ]
        ]
    "#;

        let err = AccountComponentMetadata::from_toml(toml_text).unwrap_err();
        // validate inner serde error
        assert!(err.source().unwrap().to_string().contains("are not contiguous"));
    }

    #[test]
    fn toml_fail_duplicate_storage_entry_names() {
        let toml_text = r#"
        name = "Test Component"
        description = "Component with duplicate storage entry names"
        version = "1.0.1"
        supported-types = ["FungibleFaucet"]

        [[storage]]
        # placeholder
        name = "duplicate"
        slot = 0
        type = "word"

        [[storage]]
        name = "duplicate"
        slot = 1
        value = [ "0x1", "0x1", "0x1", "0x1" ]
    "#;

        let result = AccountComponentMetadata::from_toml(toml_text);
        assert_matches::assert_matches!(
            result.unwrap_err(),
            AccountComponentTemplateError::DuplicateEntryNames(_)
        );
    }

    #[test]
    fn toml_fail_multislot_spans_one_slot() {
        let toml_text = r#"
        name = "Test Component"
        description = "Test multislot spans one slot"
        version = "1.0.1"
        supported-types = ["RegularAccountImmutableCode"]

        [[storage]]
        name = "multislot_one_slot"
        slots = [0]
        values = [
            [ "0x1", "0x2", "0x3", "0x4" ],
        ]
    "#;

        let result = AccountComponentMetadata::from_toml(toml_text);
        assert_matches::assert_matches!(
            result.unwrap_err(),
            AccountComponentTemplateError::MultiSlotSpansOneSlot
        );
    }

    #[test]
    fn test_toml_multislot_success() {
        let toml_text = r#"
        name = "Test Component"
        description = "A multi-slot success scenario"
        version = "1.0.1"
        supported-types = ["FungibleFaucet"]

        [[storage]]
        name = "multi_slot_example"
        slots = [0, 1, 2]
        values = [
            ["0x1", "0x2", "0x3", "0x4"],
            ["0x5", "0x6", "0x7", "0x8"],
            ["0x9", "0xa", "0xb", "0xc"]
        ]
    "#;

        let metadata = AccountComponentMetadata::from_toml(toml_text).unwrap();
        match &metadata.storage_entries()[0] {
            StorageEntry::MultiSlot { slots, word_entries } => match word_entries {
                crate::account::component::template::MultiWordRepresentation::Value {
                    identifier,
                    values,
                } => {
                    assert_eq!(identifier.name.as_str(), "multi_slot_example");
                    assert_eq!(slots, &(0..3));
                    assert_eq!(values.len(), 3);
                },
            },
            _ => panic!("expected multislot"),
        }
    }
}
