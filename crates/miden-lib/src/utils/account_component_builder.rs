use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use miden_objects::account::{AccountComponent, AccountType, StorageSlot};
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::assembly::{Assembler, Library, LibraryPath, Parse};

use crate::errors::AccountComponentBuilderError;
use crate::transaction::TransactionKernel;

// ACCOUNT COMPONENT BUILDER
// ================================================================================================

/// A builder for creating account components with optional library dependencies.
///
/// The AccountComponentBuilder simplifies the process of creating account components by providing:
/// - A clean API for adding multiple libraries with static or dynamic linking
/// - Automatic assembler configuration with all added libraries
/// - Debug mode support
/// - Builder pattern support for method chaining
/// - Support for setting custom storage slots and supported account types
///
/// ## Static vs Dynamic Linking
///
/// **Static Linking** (`link_static_library()` / `with_statically_linked_library()`):
/// - Use when you control and know the library code
/// - The library code is copied into the component code
/// - Best for most user-written libraries and dependencies
/// - Results in larger component size but ensures the code is always available
///
/// **Dynamic Linking** (`link_dynamic_library()` / `with_dynamically_linked_library()`):
/// - Use when making Foreign Procedure Invocation (FPI) calls
/// - The library code is available on-chain and referenced, not copied
/// - Results in smaller component size but requires the code to be available on-chain
///
/// ## Typical Workflow
///
/// 1. Create a new AccountComponentBuilder with debug mode preference
/// 2. Add any required modules using `link_module()` or `with_linked_module()`
/// 3. Add libraries using `link_static_library()` / `link_dynamic_library()` as appropriate
/// 4. Set storage slots using `with_storage_slots()` or `with_storage_slot()`
/// 5. Configure supported account types using `with_supported_type()` or `with_supported_types()`
/// 6. Build your component with `build()`
///
/// Note that the build method consumes the AccountComponentBuilder.
///
/// ## Builder Pattern Example
///
/// ```no_run
/// # use anyhow::Context;
/// # use miden_lib::utils::AccountComponentBuilder;
/// # use miden_objects::assembly::Library;
/// # use miden_objects::account::{AccountType, StorageSlot};
/// # use miden_stdlib::StdLibrary;
/// # fn example() -> anyhow::Result<()> {
/// # let module_code = "export.test push.1 add end";
/// # let component_code = "export.increment push.0 exec.account::get_item push.1 add push.0 exec.account::set_item end";
/// # // Create sample libraries for the example
/// # let my_lib = StdLibrary::default().into(); // Convert StdLibrary to Library
/// # let fpi_lib = StdLibrary::default().into();
/// let component = AccountComponentBuilder::default()
///     .with_linked_module("my::module", module_code).context("failed to link module")?
///     .with_statically_linked_library(&my_lib).context("failed to link static library")?
///     .with_dynamically_linked_library(&fpi_lib).context("failed to link dynamic library")?
///     .with_storage_slot(StorageSlot::empty_value()).context("failed to add storage slot")?
///     .with_supported_type(AccountType::RegularAccountImmutableCode)
///     .build(component_code).context("failed to build account component")?;
/// # Ok(())
/// # }
/// ```
///
/// # Note
/// The AccountComponentBuilder automatically includes the `miden` and `std` libraries, which
/// provide access to transaction kernel procedures. Due to being available on-chain
/// these libraries are linked dynamically and do not add to the size of built components.
#[derive(Clone)]
pub struct AccountComponentBuilder {
    assembler: Assembler,
    storage_slots: Vec<StorageSlot>,
    supported_types: BTreeSet<AccountType>,
}

impl AccountComponentBuilder {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new AccountComponentBuilder with the specified debug mode.
    ///
    /// This creates a basic assembler using `TransactionKernel::assembler()`.
    pub fn new(in_debug_mode: bool) -> Self {
        let assembler = TransactionKernel::assembler().with_debug_mode(in_debug_mode);
        Self {
            assembler,
            storage_slots: Vec::new(),
            supported_types: BTreeSet::new(),
        }
    }

    // LIBRARY MANAGEMENT
    // --------------------------------------------------------------------------------------------

    /// Compiles and links a module to the account component builder.
    ///
    /// This method compiles the provided module code and adds it directly to the assembler
    /// for use in component compilation.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The module path is invalid
    /// - The module code cannot be parsed
    /// - The module cannot be assembled
    pub fn link_module(
        &mut self,
        module_path: impl AsRef<str>,
        module_code: impl AsRef<str>,
    ) -> Result<(), AccountComponentBuilderError> {
        // Parse the library path
        let lib_path = LibraryPath::new(module_path.as_ref()).map_err(|err| {
            AccountComponentBuilderError::build_error_with_source(
                format!("invalid module path: {}", module_path.as_ref()),
                err,
            )
        })?;

        let module = NamedSource::new(format!("{lib_path}"), String::from(module_code.as_ref()));

        self.assembler.compile_and_statically_link(module).map_err(|err| {
            AccountComponentBuilderError::build_error_with_report("failed to assemble module", err)
        })?;

        Ok(())
    }

    /// Statically links the given library.
    ///
    /// Static linking means the library code is copied into the component code.
    /// Use this for most libraries that are not available on-chain.
    ///
    /// # Errors
    /// Returns an error if:
    /// - adding the library to the assembler failed
    pub fn link_static_library(
        &mut self,
        library: &Library,
    ) -> Result<(), AccountComponentBuilderError> {
        self.assembler.link_static_library(library).map_err(|err| {
            AccountComponentBuilderError::build_error_with_report(
                "failed to add static library",
                err,
            )
        })
    }

    /// Dynamically links a library.
    ///
    /// This is useful to dynamically link the [`Library`] of a foreign account
    /// that is invoked using foreign procedure invocation (FPI). Its code is available
    /// on-chain and so it does not have to be copied into the component code.
    ///
    /// For all other use cases not involving FPI, link the library statically.
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn link_dynamic_library(
        &mut self,
        library: &Library,
    ) -> Result<(), AccountComponentBuilderError> {
        self.assembler.link_dynamic_library(library).map_err(|err| {
            AccountComponentBuilderError::build_error_with_report(
                "failed to add dynamic library",
                err,
            )
        })
    }

    /// Builder-style method to statically link a library and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn with_statically_linked_library(
        mut self,
        library: &Library,
    ) -> Result<Self, AccountComponentBuilderError> {
        self.link_static_library(library)?;
        Ok(self)
    }

    /// Builder-style method to dynamically link a library and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn with_dynamically_linked_library(
        mut self,
        library: &Library,
    ) -> Result<Self, AccountComponentBuilderError> {
        self.link_dynamic_library(library)?;
        Ok(self)
    }

    /// Builder-style method to link a module and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Errors
    /// Returns an error if the module cannot be compiled or added to the assembler
    pub fn with_linked_module(
        mut self,
        module_path: impl AsRef<str>,
        module_code: impl AsRef<str>,
    ) -> Result<Self, AccountComponentBuilderError> {
        self.link_module(module_path, module_code)?;
        Ok(self)
    }

    // STORAGE CONFIGURATION
    // --------------------------------------------------------------------------------------------

    /// Adds a storage slot to the component.
    pub fn add_storage_slot(&mut self, slot: StorageSlot) {
        self.storage_slots.push(slot);
    }

    /// Builder-style method to add a storage slot and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    pub fn with_storage_slot(mut self, slot: StorageSlot) -> Self {
        self.add_storage_slot(slot);
        self
    }

    /// Sets the storage slots for the component, replacing any previously set slots.
    pub fn set_storage_slots(&mut self, slots: Vec<StorageSlot>) {
        self.storage_slots = slots;
    }

    /// Builder-style method to set storage slots and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    pub fn with_storage_slots(mut self, slots: Vec<StorageSlot>) -> Self {
        self.set_storage_slots(slots);
        self
    }

    // ACCOUNT TYPE CONFIGURATION
    // --------------------------------------------------------------------------------------------

    /// Adds a supported account type to the component.
    pub fn add_supported_type(&mut self, account_type: AccountType) {
        self.supported_types.insert(account_type);
    }

    /// Builder-style method to add a supported account type and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    pub fn with_supported_type(mut self, account_type: AccountType) -> Self {
        self.add_supported_type(account_type);
        self
    }

    /// Sets the supported account types for the component, replacing any previously set types.
    pub fn set_supported_types(&mut self, types: BTreeSet<AccountType>) {
        self.supported_types = types;
    }

    /// Builder-style method to set supported account types and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    pub fn with_supported_types(mut self, types: BTreeSet<AccountType>) -> Self {
        self.set_supported_types(types);
        self
    }

    /// Builder-style method to set the component to support all account types.
    pub fn with_supports_all_types(mut self) -> Self {
        self.supported_types.extend([
            AccountType::FungibleFaucet,
            AccountType::NonFungibleFaucet,
            AccountType::RegularAccountImmutableCode,
            AccountType::RegularAccountUpdatableCode,
        ]);
        self
    }

    // COMPONENT COMPILATION
    // --------------------------------------------------------------------------------------------

    /// Builds an AccountComponent with the provided source code.
    ///
    /// The built component will have access to all modules and libraries that have been added to
    /// this builder, use the configured storage slots, and support the configured account
    /// types.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The component compilation fails
    /// - The number of storage slots exceeds 255
    pub fn build(
        self,
        source_code: impl Parse,
    ) -> Result<AccountComponent, AccountComponentBuilderError> {
        let assembler = self.assembler;

        let library = assembler.assemble_library([source_code]).map_err(|err| {
            AccountComponentBuilderError::build_error_with_report(
                "failed to compile account component",
                err,
            )
        })?;

        let component = AccountComponent::new(library, self.storage_slots).map_err(|err| {
            AccountComponentBuilderError::build_error_with_source(
                "failed to create account component",
                err,
            )
        })?;

        Ok(component.with_supported_types(self.supported_types))
    }

    // TESTING CONVENIENCE FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Creates an AccountComponentBuilder with the kernel library for testing scenarios.
    ///
    /// This is equivalent to using `TransactionKernel::testing_assembler()` and is intended
    /// to replace components that were built with that assembler.
    #[cfg(any(feature = "testing", test))]
    pub fn with_kernel_library() -> Result<Self, AccountComponentBuilderError> {
        let kernel_library = TransactionKernel::kernel_as_library();
        Self::default().with_dynamically_linked_library(&kernel_library)
    }

    /// Creates an AccountComponentBuilder with both kernel and mock account libraries for testing
    /// scenarios.
    ///
    /// This is equivalent to using `TransactionKernel::testing_assembler_with_mock_account()`
    /// and is intended to replace components that were built with that assembler.
    #[cfg(any(feature = "testing", test))]
    pub fn with_mock_account_library() -> Result<Self, AccountComponentBuilderError> {
        use miden_objects::account::AccountCode;

        use crate::testing::mock_account_code::MockAccountCodeExt;

        let builder = Self::with_kernel_library()?;

        builder.with_dynamically_linked_library(&AccountCode::mock_library())
    }
}

impl Default for AccountComponentBuilder {
    fn default() -> Self {
        Self::new(true)
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use miden_objects::account::{AccountType, StorageSlot};

    use super::*;

    #[test]
    fn test_account_component_builder_new() {
        let _builder = AccountComponentBuilder::default();
        // Test that the builder can be created successfully
    }

    #[test]
    fn test_account_component_builder_basic_compilation() -> anyhow::Result<()> {
        let component_code = "
            use.miden::account
            use.std::sys
            export.increment
                push.0
                exec.account::get_item
                push.1 add
                push.0
                exec.account::set_item
                exec.sys::truncate_stack
            end
        ";

        let builder = AccountComponentBuilder::default()
            .with_storage_slot(StorageSlot::empty_value())
            .with_supported_type(AccountType::RegularAccountImmutableCode);

        let _component = builder
            .build(component_code)
            .context("failed to compile basic account component")?;
        Ok(())
    }

    #[test]
    fn test_create_library_and_build_component() -> anyhow::Result<()> {
        let component_code = "
            use.external_contract::counter_contract
            use.miden::account
            use.std::sys
            export.increment
                call.counter_contract::increment
            end
        ";

        let library_code = "
            use.miden::account
            export.increment
                push.0
                exec.account::get_item
                push.1 add
                push.0
                exec.account::set_item
            end
        ";

        let library_path = "external_contract::counter_contract";

        let mut builder_with_lib = AccountComponentBuilder::default()
            .with_storage_slot(StorageSlot::empty_value())
            .with_supported_type(AccountType::RegularAccountImmutableCode);

        builder_with_lib
            .link_module(library_path, library_code)
            .context("failed to link module")?;

        let _component =
            builder_with_lib.build(component_code).context("failed to compile component")?;

        Ok(())
    }

    #[test]
    fn test_builder_style_chaining() -> anyhow::Result<()> {
        let component_code = "
            use.external_contract::counter_contract
            use.miden::account
            use.std::sys
            export.increment
                call.counter_contract::increment
            end
        ";

        let library_code = "
            use.miden::account
            export.increment
                push.0
                exec.account::get_item
                push.1 add
                push.0
                exec.account::set_item
            end
        ";

        // Test builder-style chaining with modules
        let builder = AccountComponentBuilder::default()
            .with_linked_module("external_contract::counter_contract", library_code)
            .context("failed to link module")?
            .with_storage_slot(StorageSlot::empty_value())
            .with_supported_type(AccountType::RegularAccountImmutableCode);

        let _component = builder.build(component_code).context("failed to compile component")?;

        Ok(())
    }

    #[test]
    fn test_multiple_storage_slots_and_types() -> anyhow::Result<()> {
        let component_code = "
            use.miden::account
            use.std::sys
            export.increment
                push.0
                exec.account::get_item
                push.1 add
                push.0
                exec.account::set_item
                exec.sys::truncate_stack
            end
        ";

        let mut supported_types = BTreeSet::new();
        supported_types.insert(AccountType::RegularAccountImmutableCode);
        supported_types.insert(AccountType::RegularAccountUpdatableCode);

        let builder = AccountComponentBuilder::default()
            .with_storage_slots(vec![StorageSlot::empty_value(), StorageSlot::empty_value()])
            .with_supported_types(supported_types);

        let component = builder.build(component_code).context("failed to compile component")?;

        assert_eq!(component.storage_size(), 2);
        assert_eq!(component.supported_types().len(), 2);
        assert!(component.supports_type(AccountType::RegularAccountImmutableCode));
        assert!(component.supports_type(AccountType::RegularAccountUpdatableCode));

        Ok(())
    }

    #[test]
    fn test_supports_all_types() -> anyhow::Result<()> {
        let component_code = "
            use.miden::account
            use.std::sys
            export.increment
                push.0
                exec.account::get_item
                push.1 add
                push.0
                exec.account::set_item
                exec.sys::truncate_stack
            end
        ";

        let builder = AccountComponentBuilder::default()
            .with_storage_slot(StorageSlot::empty_value())
            .with_supports_all_types();

        let component = builder.build(component_code).context("failed to compile component")?;

        assert_eq!(component.supported_types().len(), 4);
        assert!(component.supports_type(AccountType::FungibleFaucet));
        assert!(component.supports_type(AccountType::NonFungibleFaucet));
        assert!(component.supports_type(AccountType::RegularAccountImmutableCode));
        assert!(component.supports_type(AccountType::RegularAccountUpdatableCode));

        Ok(())
    }
}
