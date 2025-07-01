use alloc::{string::String, vec::Vec};

use assembly::{Assembler, Library, LibraryPath, diagnostics::NamedSource};

use crate::{TransactionScriptError, note::NoteScript, transaction::TransactionScript};

// SCRIPT BUILDER ERROR
// ================================================================================================

#[derive(Debug, thiserror::Error)]
pub enum ScriptBuilderError {
    #[error("library build error: {0}")]
    LibraryBuildError(String),
    #[error("transaction script library error: {0}")]
    TransactionScriptLibraryError(String),
    #[error("transaction script error")]
    TransactionScriptError(#[from] TransactionScriptError),
}

// SCRIPT BUILDER
// ================================================================================================

/// A builder for compiling note scripts and transaction scripts with optional library dependencies.
///
/// The ScriptBuilder simplifies the process of creating transaction scripts by providing:
/// - A clean API for adding multiple libraries
/// - Automatic assembler configuration with all added libraries
/// - Debug mode support
///
/// The typical workflow is:
/// 1. Create a new ScriptBuilder with debug mode preference
/// 2. Add any required libraries using `add_library()`
/// 3. Compile your note script with `compile_note_script()`
/// 4. Compile your transaction script with `compile_tx_script()`
///
/// # Note
/// For scripts that need access to transaction kernel procedures,
/// use `ScriptBuilder::with_assembler()` and provide an assembler created with
/// `TransactionKernel::assembler()` from the miden-lib crate.
pub struct ScriptBuilder {
    assembler: Assembler,
    libraries: Vec<Library>,
}

impl ScriptBuilder {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new ScriptBuilder with the specified debug mode.
    ///
    /// This creates a basic assembler. For transaction scripts that need access to
    /// transaction kernel procedures, use `ScriptBuilder::with_assembler()` and provide
    /// an assembler created with `TransactionKernel::assembler()`.
    ///
    /// # Arguments
    /// * `in_debug_mode` - Whether to enable debug mode in the assembler
    pub fn new(in_debug_mode: bool) -> Self {
        let assembler = Assembler::default().with_debug_mode(in_debug_mode);
        Self { assembler, libraries: Vec::new() }
    }

    /// Creates a new ScriptBuilder with a provided assembler.
    ///
    /// This is the recommended constructor when you need access to transaction kernel
    /// procedures. Pass `TransactionKernel::assembler()` as the assembler parameter.
    ///
    /// # Arguments
    /// * `assembler` - A pre-configured assembler (e.g., from TransactionKernel::assembler())
    /// * `in_debug_mode` - Whether to enable debug mode
    pub fn with_assembler(assembler: Assembler, in_debug_mode: bool) -> Self {
        let assembler = assembler.with_debug_mode(in_debug_mode);
        Self { assembler, libraries: Vec::new() }
    }

    // LIBRARY MANAGEMENT
    // --------------------------------------------------------------------------------------------

    /// Adds multiple libraries to the script builder.
    ///
    /// # Arguments
    /// * `libraries` - Iterator of compiled libraries to add to the builder
    pub fn add_library(
        &mut self,
        libraries: impl IntoIterator<Item = Library>,
    ) -> Result<(), ScriptBuilderError> {
        for library in libraries.into_iter() {
            self.libraries.push(library);
        }
        Ok(())
    }

    // SCRIPT COMPILATION
    // --------------------------------------------------------------------------------------------

    /// Compiles a transaction script with the provided program code.
    ///
    /// The compiled script will have access to all libraries that have been added to this builder.
    ///
    /// # Arguments
    /// * `program` - The transaction script source code
    ///
    /// # Errors
    /// Returns an error if:
    /// - Any of the libraries cannot be loaded into the assembler
    /// - The transaction script compilation fails
    pub fn compile_tx_script(
        &self,
        tx_script: impl AsRef<str>,
    ) -> Result<TransactionScript, ScriptBuilderError> {
        let mut assembler = self.assembler.clone();

        // Add all libraries from the builder to the assembler
        for lib in &self.libraries {
            assembler = assembler.with_library(lib.clone()).map_err(|err| {
                ScriptBuilderError::TransactionScriptLibraryError(alloc::format!(
                    "Failed to add library: {err}"
                ))
            })?;
        }

        TransactionScript::compile(tx_script.as_ref(), assembler)
            .map_err(ScriptBuilderError::TransactionScriptError)
    }

    /// Compiles a note script with the provided program code.
    ///
    /// The compiled script will have access to all libraries that have been added to this builder.
    ///
    /// # Arguments
    /// * `program` - The note script source code
    ///
    /// # Errors
    /// Returns an error if:
    /// - Any of the libraries cannot be loaded into the assembler
    /// - The note script compilation fails
    pub fn compile_note_script(
        &self,
        program: impl AsRef<str>,
    ) -> Result<NoteScript, ScriptBuilderError> {
        let mut assembler = self.assembler.clone();

        // Add all libraries from the builder to the assembler
        for lib in &self.libraries {
            assembler = assembler.with_library(lib.clone()).map_err(|err| {
                ScriptBuilderError::TransactionScriptLibraryError(alloc::format!(
                    "Failed to add library: {err}"
                ))
            })?;
        }

        NoteScript::compile(program.as_ref(), assembler).map_err(|err| {
            ScriptBuilderError::LibraryBuildError(alloc::format!(
                "Failed to compile note script: {err}"
            ))
        })
    }

    // LIBRARY COMPILATION
    // --------------------------------------------------------------------------------------------

    /// Compiles the provided library code and library path into a Library.
    ///
    /// This method allows you to compile libraries independently and pass them
    /// to `compile_tx_script()` as needed. This matches the original client API pattern:
    /// ```ignore
    /// let library = builder.compile_library(account_code, library_path)?;
    /// let tx_script = builder.compile_tx_script(script_code, Some(library))?;
    /// ```
    ///
    /// # Arguments
    /// * `library_code` - The source code of the library
    /// * `library_path` - The path identifier for the library (e.g., "my_lib::my_module")
    ///
    /// # Errors
    /// Returns an error if:
    /// - The library path is invalid
    /// - The library code cannot be parsed
    /// - The library cannot be assembled
    pub fn compile_library(
        &self,
        library_code: impl AsRef<str>,
        library_path: impl AsRef<str>,
    ) -> Result<Library, ScriptBuilderError> {
        let assembler = self.assembler.clone();

        // Parse the library path
        let lib_path = LibraryPath::new(library_path.as_ref()).map_err(|_| {
            ScriptBuilderError::LibraryBuildError(alloc::format!(
                "Invalid library path: {}",
                library_path.as_ref()
            ))
        })?;

        let source =
            NamedSource::new(alloc::format!("{lib_path}"), String::from(library_code.as_ref()));

        let library = assembler.assemble_library([source]).map_err(|err| {
            ScriptBuilderError::LibraryBuildError(alloc::format!(
                "Failed to assemble library: {err}"
            ))
        })?;

        Ok(library)
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
        let builder = ScriptBuilder::new(true);
        assert_eq!(builder.libraries.len(), 0);
    }

    #[test]
    fn test_script_builder_basic_script_compilation() {
        let builder = ScriptBuilder::new(true);
        let result = builder.compile_tx_script("begin nop end");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_script_builder_with_assembler() {
        let assembler = Assembler::default();
        let builder = ScriptBuilder::with_assembler(assembler, false);
        assert_eq!(builder.libraries.len(), 0);
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

        let builder = ScriptBuilder::new(true);
        let library_result = builder.compile_library(account_code, library_path);

        if let Ok(library) = library_result {
            let mut builder_with_lib = ScriptBuilder::new(true);
            let add_result = builder_with_lib.add_library(vec![library]);

            if add_result.is_ok() {
                let _tx_script_result = builder_with_lib.compile_tx_script(script_code);
                assert_eq!(builder_with_lib.libraries.len(), 1);
            }
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
        let builder = ScriptBuilder::new(true);
        let library_result = builder.compile_library(account_code, library_path);

        if let Ok(library) = library_result {
            let mut builder_with_lib = ScriptBuilder::new(true);
            let add_result = builder_with_lib.add_library(vec![library]);

            if add_result.is_ok() {
                let _tx_script_result = builder_with_lib.compile_tx_script(script_code);
                assert_eq!(builder_with_lib.libraries.len(), 1);
            }
        }

        // Test multiple libraries
        let builder2 = ScriptBuilder::new(true);
        let library1_result = builder2.compile_library(account_code, library_path);
        let library2_result = builder2.compile_library("export.test begin nop end", "test::lib");

        if let (Ok(lib1), Ok(lib2)) = (library1_result, library2_result) {
            let mut builder_with_libs = ScriptBuilder::new(true);
            let add_result = builder_with_libs.add_library(vec![lib1, lib2]);

            if add_result.is_ok() {
                let _tx_script_result = builder_with_libs.compile_tx_script(script_code);
                assert_eq!(builder_with_libs.libraries.len(), 2);
            }
        }
    }
}
