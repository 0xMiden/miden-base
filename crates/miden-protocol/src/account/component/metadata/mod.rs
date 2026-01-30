use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use core::str::FromStr;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_mast_package::{Package, SectionId};
use miden_processor::DeserializationError;
use semver::Version;

use super::{AccountType, SchemaRequirement, StorageSchema, StorageValueName};
use crate::errors::AccountError;

// ACCOUNT COMPONENT METADATA BUILDER
// ================================================================================================

/// Builder for constructing [`AccountComponentMetadata`] with a fluent interface.
///
/// This builder provides a convenient way to construct component metadata with sensible defaults:
/// - Empty description
/// - Version 0.1.0
/// - Empty supported types set
/// - Empty storage schema
///
/// # Example
///
/// ```
/// use miden_protocol::account::AccountType;
/// use miden_protocol::account::component::AccountComponentMetadata;
///
/// let metadata = AccountComponentMetadata::builder("my_component::name")
///     .description("A description of the component")
///     .supports_all_types()
///     .build();
///
/// assert_eq!(metadata.name(), "my_component::name");
/// assert!(metadata.supported_types().contains(&AccountType::FungibleFaucet));
/// ```
#[derive(Debug, Clone)]
pub struct AccountComponentMetadataBuilder {
    name: String,
    description: String,
    version: Version,
    supported_types: BTreeSet<AccountType>,
    storage_schema: StorageSchema,
}

impl AccountComponentMetadataBuilder {
    /// Creates a new builder with the given component name and default values.
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: Version::new(0, 1, 0),
            supported_types: BTreeSet::new(),
            storage_schema: StorageSchema::default(),
        }
    }

    /// Sets the description of the component.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the version of the component.
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Adds a single supported account type to the component.
    pub fn supported_type(mut self, account_type: AccountType) -> Self {
        self.supported_types.insert(account_type);
        self
    }

    /// Sets the component to support all account types.
    pub fn supports_all_types(mut self) -> Self {
        self.supported_types.extend([
            AccountType::FungibleFaucet,
            AccountType::NonFungibleFaucet,
            AccountType::RegularAccountImmutableCode,
            AccountType::RegularAccountUpdatableCode,
        ]);
        self
    }

    /// Sets the component to support only regular account types (immutable and updatable code).
    pub fn supports_regular_types(mut self) -> Self {
        self.supported_types.extend([
            AccountType::RegularAccountImmutableCode,
            AccountType::RegularAccountUpdatableCode,
        ]);
        self
    }

    /// Sets the storage schema for the component.
    pub fn storage_schema(mut self, schema: StorageSchema) -> Self {
        self.storage_schema = schema;
        self
    }

    /// Builds the [`AccountComponentMetadata`].
    pub fn build(self) -> AccountComponentMetadata {
        AccountComponentMetadata::new(
            self.name,
            self.description,
            self.version,
            self.supported_types,
            self.storage_schema,
        )
    }
}

// ACCOUNT COMPONENT METADATA
// ================================================================================================

/// Represents the full component metadata configuration.
///
/// An account component metadata describes the component alongside its storage layout.
/// The storage layout can declare typed values which must be provided at instantiation time via
/// [InitStorageData](`super::storage::InitStorageData`). These can appear either at the slot level
/// (a singular word slot) or inside composed words as typed fields.
///
/// When the `std` feature is enabled, this struct allows for serialization and deserialization to
/// and from a TOML file.
///
/// # Guarantees
///
/// - The metadata's storage schema does not contain duplicate slot names.
/// - The schema cannot contain protocol-reserved slot names.
/// - Each init-time value name uniquely identifies a single value. The expected init-time metadata
///   can be retrieved with [AccountComponentMetadata::schema_requirements()], which returns a map
///   from keys to [SchemaRequirement] (which indicates the expected value type and optional
///   defaults).
///
/// # Example
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet};
///
/// use miden_protocol::account::StorageSlotName;
/// use miden_protocol::account::component::{
///     AccountComponentMetadata,
///     FeltSchema,
///     InitStorageData,
///     SchemaTypeId,
///     StorageSchema,
///     StorageSlotSchema,
///     StorageValueName,
///     ValueSlotSchema,
///     WordSchema,
///     WordValue,
/// };
/// use semver::Version;
///
/// let slot_name = StorageSlotName::new("demo::test_value")?;
///
/// let word = WordSchema::new_value([
///     FeltSchema::new_void(),
///     FeltSchema::new_void(),
///     FeltSchema::new_void(),
///     FeltSchema::new_typed(SchemaTypeId::native_felt(), "foo"),
/// ]);
///
/// let storage_schema = StorageSchema::new([(
///     slot_name.clone(),
///     StorageSlotSchema::Value(ValueSlotSchema::new(Some("demo slot".into()), word)),
/// )])?;
///
/// let metadata = AccountComponentMetadata::new(
///     "test name".into(),
///     "description of the component".into(),
///     Version::parse("0.1.0")?,
///     BTreeSet::new(),
///     storage_schema,
/// );
///
/// // Init value keys are derived from slot name: `demo::test_value.foo`.
/// let value_name = StorageValueName::from_slot_name_with_suffix(&slot_name, "foo")?;
/// let mut init_storage_data = InitStorageData::default();
/// init_storage_data.set_value(value_name, WordValue::Atomic("300".into()))?;
///
/// let storage_slots = metadata.storage_schema().build_storage_slots(&init_storage_data)?;
/// assert_eq!(storage_slots.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
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

    /// Storage schema defining the component's storage layout, defaults, and init-supplied values.
    #[cfg_attr(feature = "std", serde(rename = "storage"))]
    storage_schema: StorageSchema,
}

impl AccountComponentMetadata {
    /// Creates a new builder for [`AccountComponentMetadata`] with the given component name.
    ///
    /// See [`AccountComponentMetadataBuilder`] for more details.
    pub fn builder(name: impl Into<String>) -> AccountComponentMetadataBuilder {
        AccountComponentMetadataBuilder::new(name)
    }

    /// Create a new [AccountComponentMetadata].
    pub fn new(
        name: String,
        description: String,
        version: Version,
        targets: BTreeSet<AccountType>,
        storage_schema: StorageSchema,
    ) -> Self {
        Self {
            name,
            description,
            version,
            supported_types: targets,
            storage_schema,
        }
    }

    /// Returns the init-time values requirements for this schema.
    ///
    /// These values are used for initializing storage slot values or storage map entries. For a
    /// full example, refer to the docs for [AccountComponentMetadata].
    ///
    /// Types for returned init values are inferred based on their location in the storage layout.
    pub fn schema_requirements(&self) -> BTreeMap<StorageValueName, SchemaRequirement> {
        self.storage_schema.schema_requirements().expect("storage schema is validated")
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

    /// Returns the storage schema of the component.
    pub fn storage_schema(&self) -> &StorageSchema {
        &self.storage_schema
    }
}

impl TryFrom<&Package> for AccountComponentMetadata {
    type Error = AccountError;

    fn try_from(package: &Package) -> Result<Self, Self::Error> {
        package
            .sections
            .iter()
            .find_map(|section| {
                (section.id == SectionId::ACCOUNT_COMPONENT_METADATA).then(|| {
                    AccountComponentMetadata::read_from_bytes(&section.data).map_err(|err| {
                        AccountError::other_with_source(
                            "failed to deserialize account component metadata",
                            err,
                        )
                    })
                })
            })
            .transpose()?
            .ok_or_else(|| {
                AccountError::other(
                    "package does not contain account component metadata section - packages without explicit metadata may be intended for other purposes (e.g., note scripts, transaction scripts)",
                )
            })
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
        self.storage_schema.write_into(target);
    }
}

impl Deserializable for AccountComponentMetadata {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let name = String::read_from(source)?;
        let description = String::read_from(source)?;
        if !description.is_ascii() {
            return Err(DeserializationError::InvalidValue(
                "description must contain only ASCII characters".to_string(),
            ));
        }
        let version = semver::Version::from_str(&String::read_from(source)?)
            .map_err(|err: semver::Error| DeserializationError::InvalidValue(err.to_string()))?;
        let supported_types = BTreeSet::<AccountType>::read_from(source)?;
        let storage_schema = StorageSchema::read_from(source)?;

        Ok(Self {
            name,
            description,
            version,
            supported_types,
            storage_schema,
        })
    }
}
