use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use miden_assembly::ast::QualifiedProcedureName;
use miden_assembly::{Assembler, Library, Parse};
use miden_core::utils::Deserializable;
use miden_mast_package::Package;
use miden_processor::MastForest;

mod template;
pub use template::*;

use crate::account::{AccountType, StorageSlot};
use crate::{AccountError, Word};

/// An [`AccountComponent`] defines a [`Library`] of code and the initial value and types of
/// the [`StorageSlot`]s it accesses.
///
/// One or more components can be used to built [`AccountCode`](crate::account::AccountCode) and
/// [`AccountStorage`](crate::account::AccountStorage).
///
/// Each component is independent of other components and can only access its own storage slots.
/// Each component defines its own storage layout starting at index 0 up to the length of the
/// storage slots vector.
///
/// Components define the [`AccountType`]s they support, meaning whether the component can be used
/// to instantiate an account of that type. For example, a component implementing a fungible faucet
/// would only specify support for [`AccountType::FungibleFaucet`]. Using it to instantiate a
/// regular account would fail. By default, the set of supported types is empty, so each component
/// is forced to explicitly define what it supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountComponent {
    pub(super) library: Library,
    pub(super) storage_slots: Vec<StorageSlot>,
    pub(super) supported_types: BTreeSet<AccountType>,
}

impl AccountComponent {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new [`AccountComponent`] constructed from the provided `library` and
    /// `storage_slots`.
    ///
    /// All procedures exported from the provided code will become members of the account's public
    /// interface when added to an [`AccountCode`](crate::account::AccountCode).
    ///
    /// # Errors
    ///
    /// The following list of errors is exhaustive and can be relied upon for `expect`ing the call
    /// to this function. It is recommended that custom components ensure these conditions by design
    /// or in their fallible constructors.
    ///
    /// Returns an error if:
    /// - The number of given [`StorageSlot`]s exceeds 255.
    pub fn new(code: Library, storage_slots: Vec<StorageSlot>) -> Result<Self, AccountError> {
        // Check that we have less than 256 storage slots.
        u8::try_from(storage_slots.len())
            .map_err(|_| AccountError::StorageTooManySlots(storage_slots.len() as u64))?;

        Ok(Self {
            library: code,
            storage_slots,
            supported_types: BTreeSet::new(),
        })
    }

    /// Returns a new [`AccountComponent`] whose library is compiled from the provided `source_code`
    /// using the specified `assembler` and with the given `storage_slots`.
    ///
    /// All procedures exported from the provided code will become members of the account's public
    /// interface when added to an [`AccountCode`](crate::account::AccountCode).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the compilation of the provided source code fails.
    /// - The number of storage slots exceeds 255.
    pub fn compile(
        source_code: impl Parse,
        assembler: Assembler,
        storage_slots: Vec<StorageSlot>,
    ) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([source_code])
            .map_err(AccountError::AccountComponentAssemblyError)?;

        Self::new(library, storage_slots)
    }

    /// Instantiates an [AccountComponent] from the [AccountComponentTemplate].
    ///
    /// The template's component metadata might contain placeholders, which can be replaced by
    /// mapping storage placeholders to values through the `init_storage_data` parameter.
    ///
    /// # Errors
    ///
    /// - If any of the component's storage entries cannot be transformed into a valid storage slot.
    ///   This could be because the metadata is invalid, or storage values were not provided (or
    ///   they are not of a valid type)
    pub fn from_template(
        template: &AccountComponentTemplate,
        init_storage_data: &InitStorageData,
    ) -> Result<AccountComponent, AccountError> {
        let mut storage_slots = vec![];
        for storage_entry in template.metadata().storage_entries() {
            let entry_storage_slots = storage_entry
                .try_build_storage_slots(init_storage_data)
                .map_err(AccountError::AccountComponentTemplateInstantiationError)?;
            storage_slots.extend(entry_storage_slots);
        }

        Ok(AccountComponent::new(template.library().clone(), storage_slots)?
            .with_supported_types(template.metadata().supported_types().clone()))
    }

    /// Creates an [`AccountComponent`] from a [`Package`].
    ///
    /// This method extracts the account component metadata from the package and creates
    /// a component with the provided storage slots. The component will have default
    /// supported types (RegularAccountImmutableCode) unless overridden.
    ///
    /// # Arguments
    ///
    /// * `package` - The package containing the account component metadata and library
    /// * `storage_slots` - The storage slots to initialize the component with
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The package does not contain account component metadata
    /// - The metadata cannot be deserialized
    /// - The component creation fails (e.g., too many storage slots)
    ///
    /// # Panics
    ///
    /// Panics if the package does not contain account component metadata bytes.
    pub fn from_package(
        package: &Package,
        storage_slots: Vec<StorageSlot>,
    ) -> Result<Self, AccountError> {
        let metadata_bytes = package
            .account_component_metadata_bytes
            .as_deref()
            .ok_or(AccountError::AccountComponentMetadataMissing)?;

        let metadata =
            AccountComponentMetadata::read_from_bytes(metadata_bytes).map_err(|err| {
                AccountError::other_with_source(
                    "Failed to deserialize account component metadata",
                    err,
                )
            })?;

        let template =
            AccountComponentTemplate::new(metadata, package.unwrap_library().as_ref().clone());

        let component = AccountComponent::new(template.library().clone(), storage_slots)?;

        // Use supported types from template metadata, or default to regular account types
        let supported_types = if template.metadata().supported_types().is_empty() {
            BTreeSet::from_iter([
                AccountType::RegularAccountImmutableCode,
                AccountType::RegularAccountUpdatableCode,
            ])
        } else {
            template.metadata().supported_types().clone()
        };

        Ok(component.with_supported_types(supported_types))
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the number of storage slots accessible from this component.
    pub fn storage_size(&self) -> u8 {
        u8::try_from(self.storage_slots.len())
            .expect("storage slots len should fit in u8 per the constructor")
    }

    /// Returns a reference to the underlying [`Library`] of this component.
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Returns a reference to the underlying [`MastForest`] of this component.
    pub fn mast_forest(&self) -> &MastForest {
        self.library.mast_forest().as_ref()
    }

    /// Returns a slice of the underlying [`StorageSlot`]s of this component.
    pub fn storage_slots(&self) -> &[StorageSlot] {
        self.storage_slots.as_slice()
    }

    /// Returns a reference to the supported [`AccountType`]s.
    pub fn supported_types(&self) -> &BTreeSet<AccountType> {
        &self.supported_types
    }

    /// Returns `true` if this component supports the given `account_type`, `false` otherwise.
    pub fn supports_type(&self, account_type: AccountType) -> bool {
        self.supported_types.contains(&account_type)
    }

    /// Returns a vector of tuples (digest, is_auth) for all procedures in this component.
    pub(crate) fn get_procedures(&self) -> Vec<(Word, bool)> {
        let mut procedures = Vec::new();
        for module in self.library.module_infos() {
            for (_, procedure_info) in module.procedures() {
                let is_auth = procedure_info.name.contains("auth__");
                procedures.push((procedure_info.digest, is_auth));
            }
        }
        procedures
    }

    /// Returns the digest of the procedure with the specified name, or `None` if it was not found
    /// in this component's library or its library path is malformed.
    pub fn get_procedure_root_by_name(
        &self,
        proc_name: impl TryInto<QualifiedProcedureName>,
    ) -> Option<Word> {
        self.library.get_procedure_root_by_name(proc_name)
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Adds `supported_type` to the set of [`AccountType`]s supported by this component.
    ///
    /// This function has the semantics of [`BTreeSet::insert`], i.e. adding a type twice is fine
    /// and it can be called multiple times with different account types.
    pub fn with_supported_type(mut self, supported_type: AccountType) -> Self {
        self.supported_types.insert(supported_type);
        self
    }

    /// Overwrites any previously set supported types with the given set.
    ///
    /// This can be used to reset the supported types of a component to a chosen set, which may be
    /// useful after cloning an existing component.
    pub fn with_supported_types(mut self, supported_types: BTreeSet<AccountType>) -> Self {
        self.supported_types = supported_types;
        self
    }

    /// Sets the [`AccountType`]s supported by this component to all account types.
    pub fn with_supports_all_types(mut self) -> Self {
        self.supported_types.extend([
            AccountType::FungibleFaucet,
            AccountType::NonFungibleFaucet,
            AccountType::RegularAccountImmutableCode,
            AccountType::RegularAccountUpdatableCode,
        ]);
        self
    }
}

impl From<AccountComponent> for Library {
    fn from(component: AccountComponent) -> Self {
        component.library
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;
    use alloc::string::ToString;
    use alloc::sync::Arc;

    use miden_assembly::Assembler;
    use miden_core::utils::Serializable;
    use miden_mast_package::{MastArtifact, Package, PackageManifest};

    use super::*;
    use crate::testing::account_code::CODE;

    #[test]
    fn test_from_package() {
        // Create a simple library for testing
        let library = Assembler::default().assemble_library([CODE]).unwrap();

        // Create metadata for the component
        let metadata = AccountComponentMetadata::new(
            "test_component".to_string(),
            "A test component".to_string(),
            semver::Version::new(1, 0, 0),
            BTreeSet::from_iter([AccountType::RegularAccountImmutableCode]),
            vec![],
        )
        .unwrap();

        // Serialize the metadata
        let metadata_bytes = metadata.to_bytes();

        // Create a package with the library and metadata
        let package = Package {
            name: "test_package".to_string(),
            mast: MastArtifact::Library(Arc::new(library)),
            manifest: PackageManifest::new(None),
            account_component_metadata_bytes: Some(metadata_bytes),
        };

        // Test the from_package method
        let storage_slots = vec![];
        let component = AccountComponent::from_package(&package, storage_slots).unwrap();

        // Verify the component was created correctly
        assert_eq!(component.storage_size(), 0);
        assert!(component.supports_type(AccountType::RegularAccountImmutableCode));
        assert!(!component.supports_type(AccountType::FungibleFaucet));
    }
}
