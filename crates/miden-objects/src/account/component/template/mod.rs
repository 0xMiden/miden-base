use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str::FromStr;

use miden_assembly::Library;
use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;
use semver::Version;

use super::AccountType;
use crate::errors::AccountComponentTemplateError;

mod storage;
pub use storage::*;

// ACCOUNT COMPONENT TEMPLATE
// ================================================================================================

/// Represents a template containing a component's metadata and its associated library.
///
/// The [AccountComponentTemplate] encapsulates all necessary information to initialize and manage
/// an account component within the system. It includes the configuration details and the compiled
/// library code required for the component's operation.
///
/// A template can be instantiated into [AccountComponent](super::AccountComponent) objects.
/// The component metadata can be defined with placeholders that can be replaced at instantiation
/// time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountComponentTemplate {
    /// The component's metadata. This describes the component and how the storage is laid out,
    /// alongside how storage values are initialized.
    metadata: AccountComponentMetadata,
    /// The account component's assembled code. This defines all functionality related to the
    /// component.
    library: Library,
}

impl AccountComponentTemplate {
    /// Creates a new [AccountComponentTemplate].
    ///
    /// This template holds everything needed to describe and implement a component, including the
    /// compiled procedures (via the [Library]) and the metadata that defines the component’s
    /// storage layout ([AccountComponentMetadata]). The metadata can include storage placeholders
    /// that get filled in at the time of the [AccountComponent](super::AccountComponent)
    /// instantiation.
    pub fn new(metadata: AccountComponentMetadata, library: Library) -> Self {
        Self { metadata, library }
    }

    /// Returns a reference to the template's [AccountComponentMetadata].
    pub fn metadata(&self) -> &AccountComponentMetadata {
        &self.metadata
    }

    /// Returns a reference to the underlying [Library] of this component.
    pub fn library(&self) -> &Library {
        &self.library
    }
}

impl Serializable for AccountComponentTemplate {
    fn write_into<W: miden_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.metadata);
        target.write(&self.library);
    }
}

impl Deserializable for AccountComponentTemplate {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, miden_processor::DeserializationError> {
        // Read and deserialize the configuration from a TOML string.
        let metadata: AccountComponentMetadata = source.read()?;
        let library = Library::read_from(source)?;

        Ok(AccountComponentTemplate::new(metadata, library))
    }
}

// ACCOUNT COMPONENT METADATA
// ================================================================================================

/// Represents the full component template configuration.
///
/// An account component metadata describes the component alongside its storage layout.
/// On the storage layout, placeholders can be utilized to identify values that should be provided
/// at the moment of instantiation.
///
/// When the `std` feature is enabled, this struct allows for serialization and deserialization to
/// and from a TOML file.
///
/// # Guarantees
///
/// - The metadata's storage layout does not contain duplicate slots, and it always starts at slot
///   index 0.
/// - Storage slots are laid out in a contiguous manner.
/// - Each placeholder represents a single value. The expected placeholders can be retrieved with
///   [AccountComponentMetadata::get_placeholder_requirements()], which returns a map from keys to
///   [PlaceholderTypeRequirement] (which, in turn, indicates the expected value type for the
///   placeholder).
///
/// # Example
///
/// ```
/// # use semver::Version;
/// # use std::collections::{BTreeMap, BTreeSet};
/// # use miden_objects::{testing::account_code::CODE, account::{
/// #     AccountComponent, AccountComponentMetadata, StorageEntry,
/// #     StorageValueName,
/// #     AccountComponentTemplate, FeltRepresentation, WordRepresentation, TemplateType},
/// #     assembly::Assembler, Felt};
/// # use miden_objects::account::InitStorageData;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let first_felt = FeltRepresentation::from(Felt::new(0u64));
/// let second_felt = FeltRepresentation::from(Felt::new(1u64));
/// let third_felt = FeltRepresentation::from(Felt::new(2u64));
/// // Templated element:
/// let last_element =
///     FeltRepresentation::new_template(TemplateType::new("felt")?, StorageValueName::new("foo")?);
///
/// let word_representation = WordRepresentation::new_value(
///     [first_felt, second_felt, third_felt, last_element],
///     Some(StorageValueName::new("test_value")?.into()),
/// )
/// .with_description("this is the first entry in the storage layout");
/// let storage_entry = StorageEntry::new_value(0, word_representation);
///
/// let init_storage_data = InitStorageData::new(
///     [(StorageValueName::new("test_value.foo")?, "300".to_string())],
///     BTreeMap::new(),
/// );
///
/// let component_template = AccountComponentMetadata::new(
///     "test name".into(),
///     "description of the component".into(),
///     Version::parse("0.1.0")?,
///     BTreeSet::new(),
///     vec![storage_entry],
/// )?;
///
/// let library = Assembler::default().assemble_library([CODE]).unwrap();
/// let template = AccountComponentTemplate::new(component_template, library);
///
/// let component = AccountComponent::from_template(&template, &init_storage_data)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "kebab-case"))]
pub struct AccountComponentMetadata {
    /// The human-readable name of the component.
    name: String,

    /// A brief description of what this component is and how it works.
    description: String,

    /// The version of the component using semantic versioning.
    /// This can be used to track and manage component upgrades.
    version: Version,

    /// A set of supported target account types for this component.
    supported_types: BTreeSet<AccountType>,

    /// A list of storage entries defining the component's storage layout and initialization
    /// values.
    storage: Vec<StorageEntry>,
}

impl AccountComponentMetadata {
    /// Create a new [AccountComponentMetadata].
    ///
    /// # Errors
    ///
    /// - If the specified storage slots contain duplicates.
    /// - If the slot numbers do not start at zero.
    /// - If the slots are not contiguous.
    pub fn new(
        name: String,
        description: String,
        version: Version,
        targets: BTreeSet<AccountType>,
        storage: Vec<StorageEntry>,
    ) -> Result<Self, AccountComponentTemplateError> {
        let component = Self {
            name,
            description,
            version,
            supported_types: targets,
            storage,
        };
        component.validate()?;
        Ok(component)
    }

    /// Retrieves a map of unique storage placeholder names mapped to their expected type that
    /// require a value at the moment of component instantiation.
    ///
    /// These values will be used for initializing storage slot values, or storage map entries.
    /// For a full example on how a placeholder may be utilized, please refer to the docs for
    /// [AccountComponentMetadata].
    ///
    /// Types for the returned storage placeholders are inferred based on their location in the
    /// storage layout structure.
    pub fn get_placeholder_requirements(
        &self,
    ) -> BTreeMap<StorageValueName, PlaceholderTypeRequirement> {
        let mut templates = BTreeMap::new();
        for entry in self.storage_entries() {
            for (name, requirement) in entry.template_requirements() {
                templates.insert(name, requirement);
            }
        }

        templates
    }

    /// Returns the name of the account component.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the description of the account component.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the semantic version of the account component.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the account types supported by the component.
    pub fn supported_types(&self) -> &BTreeSet<AccountType> {
        &self.supported_types
    }

    /// Returns the list of storage entries of the component.
    pub fn storage_entries(&self) -> &Vec<StorageEntry> {
        &self.storage
    }

    /// Validate the [AccountComponentMetadata].
    ///
    /// # Errors
    ///
    /// - If the specified storage entries contain duplicate names.
    /// - If the template contains duplicate placeholder names.
    /// - If the slot numbers do not start at zero.
    /// - If the slots are not contiguous.
    fn validate(&self) -> Result<(), AccountComponentTemplateError> {
        let mut all_slots: Vec<u8> =
            self.storage.iter().flat_map(|entry| entry.slot_indices()).collect();

        // Check that slots start at 0 and are contiguous
        all_slots.sort_unstable();
        if let Some(&first_slot) = all_slots.first()
            && first_slot != 0
        {
            return Err(AccountComponentTemplateError::StorageSlotsDoNotStartAtZero(first_slot));
        }

        for slots in all_slots.windows(2) {
            if slots[1] == slots[0] {
                return Err(AccountComponentTemplateError::DuplicateSlot(slots[0]));
            }

            if slots[1] != slots[0] + 1 {
                return Err(AccountComponentTemplateError::NonContiguousSlots(slots[0], slots[1]));
            }
        }

        // Check for duplicate storage entry names
        let mut seen_names = BTreeSet::new();
        for entry in self.storage_entries() {
            entry.validate()?;
            if let Some(name) = entry.name() {
                let name_existed = !seen_names.insert(name.as_str());
                if name_existed {
                    return Err(AccountComponentTemplateError::DuplicateEntryNames(name.clone()));
                }
            }
        }

        // Check for duplicate storage placeholder names
        let mut seen_placeholder_names = BTreeSet::new();
        for entry in self.storage_entries() {
            for (name, _) in entry.template_requirements() {
                if !seen_placeholder_names.insert(name.clone()) {
                    return Err(AccountComponentTemplateError::DuplicatePlaceholderName(name));
                }
            }
        }

        Ok(())
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountComponentMetadata {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.name.write_into(target);
        self.description.write_into(target);
        self.version.to_string().write_into(target);
        self.supported_types.write_into(target);
        self.storage.write_into(target);
    }
}

impl Deserializable for AccountComponentMetadata {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        Ok(Self {
            name: String::read_from(source)?,
            description: String::read_from(source)?,
            version: semver::Version::from_str(&String::read_from(source)?).map_err(
                |err: semver::Error| DeserializationError::InvalidValue(err.to_string()),
            )?,
            supported_types: BTreeSet::<AccountType>::read_from(source)?,
            storage: Vec::<StorageEntry>::read_from(source)?,
        })
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::string::ToString;

    use assert_matches::assert_matches;
    use miden_assembly::Assembler;
    use miden_core::utils::{Deserializable, Serializable};
    use miden_core::{Felt, FieldElement};
    use semver::Version;

    use super::FeltRepresentation;
    use crate::AccountError;
    use crate::account::component::FieldIdentifier;
    use crate::account::component::template::storage::StorageEntry;
    use crate::account::component::template::{
        AccountComponentMetadata,
        AccountComponentTemplate,
        InitStorageData,
    };
    use crate::account::{AccountComponent, StorageValueName};
    use crate::errors::AccountComponentTemplateError;
    use crate::testing::account_code::CODE;

    fn default_felt_array() -> [FeltRepresentation; 4] {
        [
            FeltRepresentation::from(Felt::ZERO),
            FeltRepresentation::from(Felt::ZERO),
            FeltRepresentation::from(Felt::ZERO),
            FeltRepresentation::from(Felt::ZERO),
        ]
    }

    #[test]
    fn contiguous_value_slots() {
        let storage = vec![
            StorageEntry::new_value(0, default_felt_array()),
            StorageEntry::new_multislot(
                FieldIdentifier::with_name(StorageValueName::new("slot1").unwrap()),
                1..3,
                vec![default_felt_array(), default_felt_array()],
            ),
        ];

        let original_config = AccountComponentMetadata {
            name: "test".into(),
            description: "desc".into(),
            version: Version::parse("0.1.0").unwrap(),
            supported_types: BTreeSet::new(),
            storage,
        };

        let serialized = original_config.as_toml().unwrap();
        let deserialized = AccountComponentMetadata::from_toml(&serialized).unwrap();
        assert_eq!(deserialized, original_config);
    }

    #[test]
    fn new_non_contiguous_value_slots() {
        let storage = vec![
            StorageEntry::new_value(0, default_felt_array()),
            StorageEntry::new_value(2, default_felt_array()),
        ];

        let result = AccountComponentMetadata::new(
            "test".into(),
            "desc".into(),
            Version::parse("0.1.0").unwrap(),
            BTreeSet::new(),
            storage,
        );
        assert_matches!(result, Err(AccountComponentTemplateError::NonContiguousSlots(0, 2)));
    }

    #[test]
    fn binary_serde_roundtrip() {
        let storage = vec![
            StorageEntry::new_multislot(
                FieldIdentifier::with_name(StorageValueName::new("slot1").unwrap()),
                1..3,
                vec![default_felt_array(), default_felt_array()],
            ),
            StorageEntry::new_value(0, default_felt_array()),
        ];

        let component_metadata = AccountComponentMetadata {
            name: "test".into(),
            description: "desc".into(),
            version: Version::parse("0.1.0").unwrap(),
            supported_types: BTreeSet::new(),
            storage,
        };

        let library = Assembler::default().assemble_library([CODE]).unwrap();
        let template = AccountComponentTemplate::new(component_metadata, library);
        let _ = AccountComponent::from_template(&template, &InitStorageData::default()).unwrap();

        let serialized = template.to_bytes();
        let deserialized = AccountComponentTemplate::read_from_bytes(&serialized).unwrap();

        assert_eq!(deserialized, template);
    }

    #[test]
    pub fn fail_on_duplicate_key() {
        let toml_text = r#"
            name = "Test Component"
            description = "This is a test component"
            version = "1.0.1"
            supported-types = ["FungibleFaucet"]

            [[storage]]
            name = "map"
            description = "A storage map entry"
            slot = 0
            values = [
                { key = "0x1", value = ["0x3", "0x1", "0x2", "0x3"] },
                { key = "0x1", value = ["0x1", "0x2", "0x3", "0x10"] }
            ]
        "#;

        let result = AccountComponentMetadata::from_toml(toml_text);
        assert_matches!(result, Err(AccountComponentTemplateError::StorageMapHasDuplicateKeys(_)));
    }

    #[test]
    pub fn fail_on_duplicate_placeholder_name() {
        let toml_text = r#"
            name = "Test Component"
            description = "tests for two duplicate placeholders"
            version = "1.0.1"
            supported-types = ["FungibleFaucet"]

            [[storage]]
            name = "map"
            slot = 0
            values = [
                { key = "0x1", value = [{type = "felt", name = "test"}, "0x1", "0x2", "0x3"] },
                { key = "0x2", value = ["0x1", "0x2", "0x3", {type = "token_symbol", name = "test"}] }
            ]
        "#;

        let result = AccountComponentMetadata::from_toml(toml_text).unwrap_err();
        assert_matches::assert_matches!(
            result,
            AccountComponentTemplateError::DuplicatePlaceholderName(_)
        );
    }

    #[test]
    pub fn fail_duplicate_key_instance() {
        let _ = color_eyre::install();

        let toml_text = r#"
            name = "Test Component"
            description = "This is a test component"
            version = "1.0.1"
            supported-types = ["FungibleFaucet"]

            [[storage]]
            name = "map"
            description = "A storage map entry"
            slot = 0
            values = [
                { key = ["0", "0", "0", "1"], value = ["0x9", "0x12", "0x31", "0x18"] },
                { key = { name="duplicate_key" }, value = ["0x1", "0x2", "0x3", "0x4"] }
            ]
        "#;

        let metadata = AccountComponentMetadata::from_toml(toml_text).unwrap();
        let library = Assembler::default().assemble_library([CODE]).unwrap();
        let template = AccountComponentTemplate::new(metadata, library);

        // Fail to instantiate on a duplicate key

        let init_storage_data = InitStorageData::new(
            [(
                StorageValueName::new("map.duplicate_key").unwrap(),
                "0x0000000000000000000000000000000000000000000000000100000000000000".to_string(),
            )],
            BTreeMap::new(),
        );
        let account_component = AccountComponent::from_template(&template, &init_storage_data);
        assert_matches!(
            account_component,
            Err(AccountError::AccountComponentTemplateInstantiationError(
                AccountComponentTemplateError::StorageMapHasDuplicateKeys(_)
            ))
        );

        // Successfully instantiate a map (keys are not duplicate)
        let valid_init_storage_data = InitStorageData::new(
            [(StorageValueName::new("map.duplicate_key").unwrap(), "0x30".to_string())],
            BTreeMap::new(),
        );
        AccountComponent::from_template(&template, &valid_init_storage_data).unwrap();
    }
}
