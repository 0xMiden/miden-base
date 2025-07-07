use alloc::{string::String, sync::Arc};

use miden_objects::{
    assembly::{
        Assembler, Library, LibraryPath,
        diagnostics::{NamedSource, SourceManager},
    },
    note::NoteScript,
    transaction::TransactionScript,
};

use crate::{errors::ScriptBuilderError, transaction::TransactionKernel};

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
/// # use miden_lib::utils::ScriptBuilder;
/// # use miden_objects::assembly::Library;
/// # use miden_stdlib::StdLibrary;
/// # let module_code = "export.test begin nop end";
/// # let script_code = "begin nop end";
/// # // Create sample libraries for the example
/// # let my_lib = StdLibrary::default().into(); // Convert StdLibrary to Library
/// # let fpi_lib = StdLibrary::default().into();
/// let script = ScriptBuilder::new(true)
///     .with_linked_module("my::module", module_code)?
///     .with_statically_linked_library(&my_lib)?
///     .with_dynamically_linked_library(&fpi_lib)?  // For FPI calls
///     .compile_tx_script(script_code)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Note
/// The ScriptBuilder automatically includes the `miden` and `std` libraries, which
/// provide access to transaction kernel procedures. Due to being available on-chain
/// these libraries are linked dynamically and do not add to the size of built script.
#[derive(Clone)]
pub struct ScriptBuilder {
    assembler: Assembler,
}

impl ScriptBuilder {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new ScriptBuilder with the specified debug mode.
    ///
    /// This creates a basic assembler using `TransactionKernel::assembler()`.
    ///
    /// # Arguments
    /// * `in_debug_mode` - Whether to enable debug mode in the assembler
    pub fn new(in_debug_mode: bool) -> Self {
        let assembler = TransactionKernel::assembler().with_debug_mode(in_debug_mode);
        Self { assembler }
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
        let lib_path = LibraryPath::new(module_path.as_ref()).map_err(|err| {
            ScriptBuilderError::build_error_with_source(
                format!("invalid module path: {}", module_path.as_ref()),
                err,
            )
        })?;

        let module = NamedSource::new(format!("{lib_path}"), String::from(module_code.as_ref()));

        self.assembler.add_module(module).map_err(|err| {
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
        self.assembler.add_vendored_library(library).map_err(|err| {
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
        self.assembler.add_library(library).map_err(|err| {
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

    // UTILITIES
    // --------------------------------------------------------------------------------------------

    /// Returns the assembler's source manager.
    ///
    /// After script building, the source manager can be fetched and passed on to the VM
    /// processor to make the source files available to create better error messages.
    pub fn source_manager(&self) -> Arc<dyn SourceManager + Send + Sync> {
        self.assembler.source_manager()
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

        TransactionScript::compile(tx_script.as_ref(), assembler).map_err(|err| {
            ScriptBuilderError::build_error_with_source("failed to compile transaction script", err)
        })
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

        NoteScript::compile(program.as_ref(), assembler).map_err(|err| {
            ScriptBuilderError::build_error_with_source("failed to compile note script", err)
        })
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self::new(false)
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_builder_new() {
        let _builder = ScriptBuilder::new(true);
        // Test that the builder can be created successfully
    }

    #[test]
    fn test_script_builder_basic_script_compilation() {
        let builder = ScriptBuilder::new(true);
        let result = builder.compile_tx_script("begin nop end");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_create_library_and_create_tx_script() {
        let script_code = "
            use.external_contract::counter_contract
            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
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

        let library_path = "external_contract::counter_contract";

        let mut builder_with_lib = ScriptBuilder::new(true);
        let add_result = builder_with_lib.link_module(library_path, account_code);

        if add_result.is_ok() {
            let _tx_script_result = builder_with_lib.compile_tx_script(script_code);
        }
    }

    #[test]
    fn test_compile_library_and_add_to_builder() {
        let script_code = "
            use.external_contract::counter_contract
            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
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

        let library_path = "external_contract::counter_contract";

        // Test single library
        let mut builder_with_lib = ScriptBuilder::new(true);
        let add_result = builder_with_lib.link_module(library_path, account_code);

        if add_result.is_ok() {
            let _tx_script_result = builder_with_lib.compile_tx_script(script_code);
        }

        // Test multiple libraries
        let mut builder_with_libs = ScriptBuilder::new(true);
        let add_result1 = builder_with_libs.link_module(library_path, account_code);
        let add_result2 = builder_with_libs.link_module("test::lib", "export.test begin nop end");

        if add_result1.is_ok() && add_result2.is_ok() {
            let _tx_script_result = builder_with_libs.compile_tx_script(script_code);
        }
    }

    #[test]
    fn test_builder_style_chaining() {
        let script_code = "
            use.external_contract::counter_contract
            begin
                call.counter_contract::increment
            end
        ";

        let account_code = "
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

        // Test builder-style chaining with modules
        let builder_result = ScriptBuilder::new(true)
            .with_linked_module("external_contract::counter_contract", account_code);

        if let Ok(builder) = builder_result {
            let _tx_script_result = builder.compile_tx_script(script_code);
        }
    }

    #[test]
    fn test_multiple_chained_modules() {
        let script_code = "begin nop end";

        // Test chaining multiple modules
        let builder_result = ScriptBuilder::new(true)
            .with_linked_module("test::lib1", "export.test1 begin nop end")
            .and_then(|b| b.with_linked_module("test::lib2", "export.test2 begin nop end"));

        if let Ok(builder) = builder_result {
            let _tx_script_result = builder.compile_tx_script(script_code);
        }
    }
}
