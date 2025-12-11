use alloc::string::String;
use alloc::sync::Arc;

use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::assembly::{Assembler, DefaultSourceManager, Library, Path, SourceManagerSync};
use miden_objects::note::NoteScript;
use miden_objects::transaction::TransactionScript;

use crate::errors::ScriptBuilderError;
use crate::transaction::TransactionKernel;

// SCRIPT BUILDER
// ================================================================================================

/// A builder for compiling note scripts and transaction scripts with optional library dependencies.
///
/// The ScriptBuilder simplifies the process of creating transaction scripts by providing:
/// - A clean API for adding multiple libraries with static or dynamic linking
/// - Automatic assembler configuration with all added libraries
/// - Debug mode support
/// - Builder pattern support for method chaining
///
/// ## Static vs Dynamic Linking
///
/// **Static Linking** (`link_static_library()` / `with_statically_linked_library()`):
/// - Use when you control and know the library code
/// - The library code is copied into the script code
/// - Best for most user-written libraries and dependencies
/// - Results in larger script size but ensures the code is always available
///
/// **Dynamic Linking** (`link_dynamic_library()` / `with_dynamically_linked_library()`):
/// - Use when making Foreign Procedure Invocation (FPI) calls
/// - The library code is available on-chain and referenced, not copied
/// - Results in smaller script size but requires the code to be available on-chain
///
/// ## Typical Workflow
///
/// 1. Create a new ScriptBuilder with debug mode preference
/// 2. Add any required modules using `link_module()` or `with_linked_module()`
/// 3. Add libraries using `link_static_library()` / `link_dynamic_library()` as appropriate
/// 4. Compile your script with `compile_note_script()` or `compile_tx_script()`
///
/// Note that the compilation methods consume the ScriptBuilder, so if you need to compile
/// multiple scripts with the same configuration, you should clone the builder first.
///
/// ## Builder Pattern Example
///
/// ```no_run
/// # use anyhow::Context;
/// # use miden_lib::utils::ScriptBuilder;
/// # use miden_objects::assembly::Library;
/// # use miden_core_lib::CoreLibrary;
/// # fn example() -> anyhow::Result<()> {
/// # let module_code = "pub proc test push.1 add end";
/// # let script_code = "begin nop end";
/// # // Create sample libraries for the example
/// # let my_lib = StdLibrary::default().into(); // Convert StdLibrary to Library
/// # let fpi_lib = StdLibrary::default().into();
/// let script = ScriptBuilder::default()
///     .with_linked_module("my::module", module_code).context("failed to link module")?
///     .with_statically_linked_library(&my_lib).context("failed to link static library")?
///     .with_dynamically_linked_library(&fpi_lib).context("failed to link dynamic library")?  // For FPI calls
///     .compile_tx_script(script_code).context("failed to compile tx script")?;
/// # Ok(())
/// # }
/// ```
///
/// # Note
/// The ScriptBuilder automatically includes the `miden` and `std` libraries, which
/// provide access to transaction kernel procedures. Due to being available on-chain
/// these libraries are linked dynamically and do not add to the size of built script.
#[derive(Clone)]
pub struct ScriptBuilder {
    assembler: Assembler,
    source_manager: Arc<dyn SourceManagerSync>,
}

impl ScriptBuilder {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new ScriptBuilder.
    ///
    /// TODO: remove `in_debug_mode` argument
    pub fn new(_in_debug_mode: bool) -> Self {
        let source_manager = Arc::new(DefaultSourceManager::default());
        let assembler = TransactionKernel::assembler_with_source_manager(source_manager.clone());
        Self { assembler, source_manager }
    }

    /// Creates a new ScriptBuilder with the specified source manager.
    ///
    /// TODO: remove `in_debug_mode` argument
    ///
    /// # Arguments
    /// * `source_manager` - The source manager to use with the internal `Assembler`
    pub fn with_source_manager(source_manager: Arc<dyn SourceManagerSync>) -> Self {
        let assembler = TransactionKernel::assembler_with_source_manager(source_manager.clone());
        Self { assembler, source_manager }
    }

    // LIBRARY MANAGEMENT
    // --------------------------------------------------------------------------------------------

    /// Compiles and links a module to the script builder.
    ///
    /// This method compiles the provided module code and adds it directly to the assembler
    /// for use in script compilation.
    ///
    /// # Arguments
    /// * `module_path` - The path identifier for the module (e.g., "my_lib::my_module")
    /// * `module_code` - The source code of the module to compile and link
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
    ) -> Result<(), ScriptBuilderError> {
        // Parse the library path
        let lib_path = Path::new(&module_path);

        let module = NamedSource::new(format!("{lib_path}"), String::from(module_code.as_ref()));

        self.assembler.compile_and_statically_link(module).map_err(|err| {
            ScriptBuilderError::build_error_with_report("failed to assemble module", err)
        })?;

        Ok(())
    }

    /// Statically links the given library.
    ///
    /// Static linking means the library code is copied into the script code.
    /// Use this for most libraries that are not available on-chain.
    ///
    /// # Arguments
    /// * `library` - The compiled library to statically link
    ///
    /// # Errors
    /// Returns an error if:
    /// - adding the library to the assembler failed
    pub fn link_static_library(&mut self, library: &Library) -> Result<(), ScriptBuilderError> {
        self.assembler.link_static_library(library).map_err(|err| {
            ScriptBuilderError::build_error_with_report("failed to add static library", err)
        })
    }

    /// Dynamically links a library.
    ///
    /// This is useful to dynamically link the [`Library`] of a foreign account
    /// that is invoked using foreign procedure invocation (FPI). Its code is available
    /// on-chain and so it does not have to be copied into the script code.
    ///
    /// For all other use cases not involving FPI, link the library statically.
    ///
    /// # Arguments
    /// * `library` - The compiled library to dynamically link
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn link_dynamic_library(&mut self, library: &Library) -> Result<(), ScriptBuilderError> {
        self.assembler.link_dynamic_library(library).map_err(|err| {
            ScriptBuilderError::build_error_with_report("failed to add dynamic library", err)
        })
    }

    /// Builder-style method to statically link a library and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Arguments
    /// * `library` - The compiled library to statically link
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn with_statically_linked_library(
        mut self,
        library: &Library,
    ) -> Result<Self, ScriptBuilderError> {
        self.link_static_library(library)?;
        Ok(self)
    }

    /// Builder-style method to dynamically link a library and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Arguments
    /// * `library` - The compiled library to dynamically link
    ///
    /// # Errors
    /// Returns an error if the library cannot be added to the assembler
    pub fn with_dynamically_linked_library(
        mut self,
        library: &Library,
    ) -> Result<Self, ScriptBuilderError> {
        self.link_dynamic_library(library)?;
        Ok(self)
    }

    /// Builder-style method to link a module and return the modified builder.
    ///
    /// This enables method chaining for convenient builder patterns.
    ///
    /// # Arguments
    /// * `module_path` - The path identifier for the module (e.g., "my_lib::my_module")
    /// * `module_code` - The source code of the module to compile and link
    ///
    /// # Errors
    /// Returns an error if the module cannot be compiled or added to the assembler
    pub fn with_linked_module(
        mut self,
        module_path: impl AsRef<str>,
        module_code: impl AsRef<str>,
    ) -> Result<Self, ScriptBuilderError> {
        self.link_module(module_path, module_code)?;
        Ok(self)
    }

    // SCRIPT COMPILATION
    // --------------------------------------------------------------------------------------------

    /// Compiles a transaction script with the provided program code.
    ///
    /// The compiled script will have access to all modules that have been added to this builder.
    ///
    /// # Arguments
    /// * `program` - The transaction script source code
    ///
    /// # Errors
    /// Returns an error if:
    /// - The transaction script compilation fails
    pub fn compile_tx_script(
        self,
        tx_script: impl AsRef<str>,
    ) -> Result<TransactionScript, ScriptBuilderError> {
        let assembler = self.assembler;

        let program = assembler.assemble_program(tx_script.as_ref()).map_err(|err| {
            ScriptBuilderError::build_error_with_report("failed to compile transaction script", err)
        })?;
        Ok(TransactionScript::new(program))
    }

    /// Compiles a note script with the provided program code.
    ///
    /// The compiled script will have access to all modules that have been added to this builder.
    ///
    /// # Arguments
    /// * `program` - The note script source code
    ///
    /// # Errors
    /// Returns an error if:
    /// - The note script compilation fails
    pub fn compile_note_script(
        self,
        program: impl AsRef<str>,
    ) -> Result<NoteScript, ScriptBuilderError> {
        let assembler = self.assembler;

        let program = assembler.assemble_program(program.as_ref()).map_err(|err| {
            ScriptBuilderError::build_error_with_report("failed to compile note script", err)
        })?;
        Ok(NoteScript::new(program))
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Access the [`Assembler`]'s [`SourceManagerSync`].
    pub fn source_manager(&self) -> Arc<dyn SourceManagerSync> {
        self.source_manager.clone()
    }

    // TESTING CONVENIENCE FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Returns a [`ScriptBuilder`] with the `mock::{account, faucet, util}` libraries.
    ///
    /// This script builder includes:
    /// - [`MockAccountCodeExt::mock_account_library`][account_lib],
    /// - [`MockAccountCodeExt::mock_faucet_library`][faucet_lib],
    /// - [`mock_util_library`][util_lib]
    ///
    /// [account_lib]: crate::testing::mock_account_code::MockAccountCodeExt::mock_account_library
    /// [faucet_lib]: crate::testing::mock_account_code::MockAccountCodeExt::mock_faucet_library
    /// [util_lib]: crate::testing::mock_util_lib::mock_util_library
    #[cfg(any(feature = "testing", test))]
    pub fn with_mock_libraries() -> Result<Self, ScriptBuilderError> {
        use miden_objects::account::AccountCode;

        use crate::testing::mock_account_code::MockAccountCodeExt;
        use crate::testing::mock_util_lib::mock_util_library;

        Self::new(true)
            .with_dynamically_linked_library(&AccountCode::mock_account_library())?
            .with_dynamically_linked_library(&AccountCode::mock_faucet_library())?
            .with_statically_linked_library(&mock_util_library())
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self::new(true)
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    #[test]
    fn test_script_builder_new() {
        let _builder = ScriptBuilder::default();
        // Test that the builder can be created successfully
    }

    #[test]
    fn test_script_builder_basic_script_compilation() -> anyhow::Result<()> {
        let builder = ScriptBuilder::default();
        builder
            .compile_tx_script("begin nop end")
            .context("failed to compile basic tx script")?;
        Ok(())
    }

    #[test]
    fn test_create_library_and_create_tx_script() -> anyhow::Result<()> {
        let script_code = "
            use external_contract::counter_contract

            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
            use miden::active_account
            use miden::native_account
            use miden::core::sys

            pub proc increment
                push.0
                exec.active_account::get_item
                push.1 add
                push.0
                exec.native_account::set_item
                exec.sys::truncate_stack
            end
        ";

        let library_path = "external_contract::counter_contract";

        let mut builder_with_lib = ScriptBuilder::default();
        builder_with_lib
            .link_module(library_path, account_code)
            .context("failed to link module")?;
        builder_with_lib
            .compile_tx_script(script_code)
            .context("failed to compile tx script")?;

        Ok(())
    }

    #[test]
    fn test_compile_library_and_add_to_builder() -> anyhow::Result<()> {
        let script_code = "
            use external_contract::counter_contract

            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
            use miden::active_account
            use miden::native_account
            use miden::core::sys

            pub proc increment
                push.0
                exec.active_account::get_item
                push.1 add
                push.0
                exec.native_account::set_item
                exec.sys::truncate_stack
            end
        ";

        let library_path = "external_contract::counter_contract";

        // Test single library
        let mut builder_with_lib = ScriptBuilder::default();
        builder_with_lib
            .link_module(library_path, account_code)
            .context("failed to link module")?;
        builder_with_lib
            .compile_tx_script(script_code)
            .context("failed to compile tx script")?;

        // Test multiple libraries
        let mut builder_with_libs = ScriptBuilder::default();
        builder_with_libs
            .link_module(library_path, account_code)
            .context("failed to link first module")?;
        builder_with_libs
            .link_module("test::lib", "pub proc test nop end")
            .context("failed to link second module")?;
        builder_with_libs
            .compile_tx_script(script_code)
            .context("failed to compile tx script with multiple libraries")?;

        Ok(())
    }

    #[test]
    fn test_builder_style_chaining() -> anyhow::Result<()> {
        let script_code = "
            use external_contract::counter_contract

            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
            use miden::active_account
            use miden::native_account
            use miden::core::sys

            pub proc increment
                push.0
                exec.active_account::get_item
                push.1 add
                push.0
                exec.native_account::set_item
                exec.sys::truncate_stack
            end
        ";

        // Test builder-style chaining with modules
        let builder = ScriptBuilder::default()
            .with_linked_module("external_contract::counter_contract", account_code)
            .context("failed to link module")?;

        builder.compile_tx_script(script_code).context("failed to compile tx script")?;

        Ok(())
    }

    #[test]
    fn test_multiple_chained_modules() -> anyhow::Result<()> {
        let script_code =
            "use test::lib1 use test::lib2 begin exec.lib1::test1 exec.lib2::test2 end";

        // Test chaining multiple modules
        let builder = ScriptBuilder::default()
            .with_linked_module("test::lib1", "pub proc test1 push.1 add end")
            .context("failed to link first module")?
            .with_linked_module("test::lib2", "pub proc test2 push.2 add end")
            .context("failed to link second module")?;

        builder.compile_tx_script(script_code).context("failed to compile tx script")?;

        Ok(())
    }

    #[test]
    fn test_static_and_dynamic_linking() -> anyhow::Result<()> {
        let script_code = "
            use external_contract::contract_1
            use external_contract::contract_2

            begin
                call.contract_1::increment_1
                call.contract_2::increment_2
            end
        ";

        let account_code_1 = "
            use miden::active_account
            use miden::native_account
            use miden::core::sys

            pub proc increment_1
                push.0
                exec.active_account::get_item
                push.1 add
                push.0
                exec.native_account::set_item
                exec.sys::truncate_stack
            end
        ";

        let account_code_2 = "
            use miden::active_account
            use miden::native_account
            use miden::core::sys

            pub proc increment_2
                push.0
                exec.active_account::get_item
                push.2 add
                push.0
                exec.native_account::set_item
                exec.sys::truncate_stack
            end
        ";

        // Create libraries using the assembler
        let temp_assembler = TransactionKernel::assembler();

        let static_lib = temp_assembler
            .clone()
            .assemble_library([NamedSource::new("external_contract::contract_1", account_code_1)])
            .map_err(|e| anyhow::anyhow!("failed to assemble static library: {}", e))?;

        let dynamic_lib = temp_assembler
            .assemble_library([NamedSource::new("external_contract::contract_2", account_code_2)])
            .map_err(|e| anyhow::anyhow!("failed to assemble dynamic library: {}", e))?;

        // Test linking both static and dynamic libraries
        let builder = ScriptBuilder::default()
            .with_statically_linked_library(&static_lib)
            .context("failed to link static library")?
            .with_dynamically_linked_library(&dynamic_lib)
            .context("failed to link dynamic library")?;

        builder
            .compile_tx_script(script_code)
            .context("failed to compile tx script with static and dynamic libraries")?;

        Ok(())
    }
}
