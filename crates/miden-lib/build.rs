#![allow(clippy::uninlined_format_args)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fmt::Write,
    fs::{self},
    io::{self},
    path::{Path, PathBuf},
    sync::Arc,
};

use assembly::{
    Assembler, DefaultSourceManager, KernelLibrary, Library, LibraryNamespace, Report,
    diagnostics::{IntoDiagnostic, NamedSource, Result, WrapErr},
    utils::Serializable,
};
use regex::Regex;
use walkdir::WalkDir;

/// A map where the key is the error name and the value is the error code with the message.
type ErrorCategoryMap = BTreeMap<ErrorCategory, Vec<NamedError>>;

// CONSTANTS
// ================================================================================================

/// Defines whether the build script should generate files in `/src`.
/// The docs.rs build pipeline has a read-only filesystem, so we have to avoid writing to `src`,
/// otherwise the docs will fail to build there. Note that writing to `OUT_DIR` is fine.
const BUILD_GENERATED_FILES_IN_SRC: bool = option_env!("BUILD_GENERATED_FILES_IN_SRC").is_some();

const ASSETS_DIR: &str = "assets";
const ASM_DIR: &str = "asm";
const ASM_MIDEN_DIR: &str = "miden";
const ASM_NOTE_SCRIPTS_DIR: &str = "note_scripts";
const ASM_ACCOUNT_COMPONENTS_DIR: &str = "account_components";
const SHARED_DIR: &str = "shared";
const ASM_TX_KERNEL_DIR: &str = "kernels/transaction";
const KERNEL_V0_RS_FILE: &str = "src/transaction/procedures/kernel_v0.rs";

const TX_KERNEL_ERRORS_FILE: &str = "src/errors/tx_kernel_errors.rs";
const NOTE_SCRIPT_ERRORS_FILE: &str = "src/errors/note_script_errors.rs";

const TX_KERNEL_ERRORS_ARRAY_NAME: &str = "TX_KERNEL_ERRORS";
const NOTE_SCRIPT_ERRORS_ARRAY_NAME: &str = "NOTE_SCRIPT_ERRORS";

const TX_KERNEL_ERROR_CATEGORIES: [TxKernelErrorCategory; 11] = [
    TxKernelErrorCategory::Kernel,
    TxKernelErrorCategory::Prologue,
    TxKernelErrorCategory::Epilogue,
    TxKernelErrorCategory::Tx,
    TxKernelErrorCategory::Note,
    TxKernelErrorCategory::Account,
    TxKernelErrorCategory::ForeignAccount,
    TxKernelErrorCategory::Faucet,
    TxKernelErrorCategory::FungibleAsset,
    TxKernelErrorCategory::NonFungibleAsset,
    TxKernelErrorCategory::Vault,
];

// PRE-PROCESSING
// ================================================================================================

/// Read and parse the contents from `./asm`.
/// - Compiles contents of asm/miden directory into a Miden library file (.masl) under miden
///   namespace.
/// - Compiles contents of asm/scripts directory into individual .masb files.
fn main() -> Result<()> {
    // re-build when the MASM code changes
    println!("cargo:rerun-if-changed={ASM_DIR}");
    println!("cargo::rerun-if-env-changed=BUILD_GENERATED_FILES_IN_SRC");

    // Copies the MASM code to the build directory
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_dir = env::var("OUT_DIR").unwrap();
    let src = Path::new(&crate_dir).join(ASM_DIR);
    let dst = Path::new(&build_dir).to_path_buf();
    copy_directory(src, &dst);

    // set source directory to {OUT_DIR}/asm
    let source_dir = dst.join(ASM_DIR);

    // set target directory to {OUT_DIR}/assets
    let target_dir = Path::new(&build_dir).join(ASSETS_DIR);

    // compile transaction kernel
    let mut assembler =
        compile_tx_kernel(&source_dir.join(ASM_TX_KERNEL_DIR), &target_dir.join("kernels"))?;

    // compile miden library
    let miden_lib = compile_miden_lib(&source_dir, &target_dir, assembler.clone())?;
    assembler.add_library(miden_lib)?;

    // compile note scripts
    compile_note_scripts(
        &source_dir.join(ASM_NOTE_SCRIPTS_DIR),
        &target_dir.join(ASM_NOTE_SCRIPTS_DIR),
        assembler.clone(),
    )?;

    // compile account components
    compile_account_components(
        &source_dir.join(ASM_ACCOUNT_COMPONENTS_DIR),
        &target_dir.join(ASM_ACCOUNT_COMPONENTS_DIR),
        assembler,
    )?;

    generate_error_constants(&source_dir)?;

    Ok(())
}

// COMPILE TRANSACTION KERNEL
// ================================================================================================

/// Reads the transaction kernel MASM source from the `source_dir`, compiles it, saves the results
/// to the `target_dir`, and returns an [Assembler] instantiated with the compiled kernel.
///
/// Additionally it compiles the transaction script executor program, see the
/// [compile_tx_script_main] procedure for details.
///
/// `source_dir` is expected to have the following structure:
///
/// - {source_dir}/api.masm       -> defines exported procedures from the transaction kernel.
/// - {source_dir}/main.masm      -> defines the executable program of the transaction kernel.
/// - {source_dir}/tx_script_main -> defines the executable program of the arbitrary transaction
///   script.
/// - {source_dir}/lib            -> contains common modules used by both api.masm and main.masm.
///
/// The compiled files are written as follows:
///
/// - {target_dir}/tx_kernel.masl             -> contains kernel library compiled from api.masm.
/// - {target_dir}/tx_kernel.masb             -> contains the executable compiled from main.masm.
/// - {target_dir}/tx_script_main.masb        -> contains the executable compiled from
///   tx_script_main.masm.
/// - src/transaction/procedures/kernel_v0.rs -> contains the kernel procedures table.
fn compile_tx_kernel(source_dir: &Path, target_dir: &Path) -> Result<Assembler> {
    let shared_path = Path::new(ASM_DIR).join(SHARED_DIR);
    let kernel_namespace = LibraryNamespace::new("kernel").expect("namespace should be valid");

    let mut assembler = build_assembler(None)?;
    // add the shared modules to the kernel lib under the kernel::util namespace
    assembler.add_modules_from_dir(kernel_namespace.clone(), &shared_path)?;

    // assemble the kernel library and write it to the "tx_kernel.masl" file
    let kernel_lib = KernelLibrary::from_dir(
        source_dir.join("api.masm"),
        Some(source_dir.join("lib")),
        assembler,
    )?;

    // generate `kernel_v0.rs` file
    generate_kernel_proc_hash_file(kernel_lib.clone())?;

    let output_file = target_dir.join("tx_kernel").with_extension(Library::LIBRARY_EXTENSION);
    kernel_lib.write_to_file(output_file).into_diagnostic()?;

    let assembler = build_assembler(Some(kernel_lib))?;

    // assemble the kernel program and write it to the "tx_kernel.masb" file
    let mut main_assembler = assembler.clone();
    // add the shared modules to the kernel lib under the kernel::util namespace
    main_assembler.add_modules_from_dir(kernel_namespace.clone(), &shared_path)?;
    main_assembler.add_modules_from_dir(kernel_namespace, &source_dir.join("lib"))?;

    let main_file_path = source_dir.join("main.masm");
    let kernel_main = main_assembler.clone().assemble_program(main_file_path)?;

    let masb_file_path = target_dir.join("tx_kernel.masb");
    kernel_main.write_to_file(masb_file_path).into_diagnostic()?;

    // compile the transaction script main program
    compile_tx_script_main(source_dir, target_dir, main_assembler)?;

    #[cfg(any(feature = "testing", test))]
    {
        let mut kernel_lib_assembler = assembler.clone();
        // Build kernel as a library and save it to file.
        // This is needed in test assemblers to access individual procedures which would otherwise
        // be hidden when using KernelLibrary (api.masm)
        let kernel_namespace =
            "kernel".parse::<LibraryNamespace>().expect("invalid base namespace");

        // add the shared modules to the kernel lib under the kernel::util namespace
        kernel_lib_assembler.add_modules_from_dir(kernel_namespace.clone(), &shared_path)?;

        let test_lib =
            Library::from_dir(source_dir.join("lib"), kernel_namespace, kernel_lib_assembler)
                .unwrap();

        let masb_file_path =
            target_dir.join("kernel_library").with_extension(Library::LIBRARY_EXTENSION);
        test_lib.write_to_file(masb_file_path).into_diagnostic()?;
    }

    Ok(assembler)
}

/// Reads the transaction script executor MASM source from the `source_dir/tx_script_main.masm`,
/// compiles it and saves the results to the `target_dir` as a `tx_script_main.masb` binary file.
fn compile_tx_script_main(
    source_dir: &Path,
    target_dir: &Path,
    main_assembler: Assembler,
) -> Result<()> {
    // assemble the transaction script executor program and write it to the "tx_script_main.masb"
    // file.
    let tx_script_main_file_path = source_dir.join("tx_script_main.masm");
    let tx_script_main = main_assembler.assemble_program(tx_script_main_file_path)?;

    let masb_file_path = target_dir.join("tx_script_main.masb");
    tx_script_main.write_to_file(masb_file_path).into_diagnostic()
}

/// Generates `kernel_v0.rs` file based on the kernel library
fn generate_kernel_proc_hash_file(kernel: KernelLibrary) -> Result<()> {
    // Because the kernel Rust file will be stored under ./src, this should be a no-op if we can't
    // write there
    if !BUILD_GENERATED_FILES_IN_SRC {
        return Ok(());
    }

    let (_, module_info, _) = kernel.into_parts();

    let to_exclude = BTreeSet::from_iter(["exec_kernel_proc"]);
    let offsets_filename = Path::new(ASM_DIR).join(ASM_MIDEN_DIR).join("kernel_proc_offsets.masm");
    let offsets = parse_proc_offsets(&offsets_filename)?;
    let generated_procs: BTreeMap<usize, String> = module_info
        .procedures()
        .filter(|(_, proc_info)| !to_exclude.contains::<str>(proc_info.name.as_ref()))
        .map(|(_, proc_info)| {
            let name = proc_info.name.to_string();

            let Some(&offset) = offsets.get(&name) else {
                panic!("Offset constant for function `{name}` not found in `{offsets_filename:?}`");
            };

            (offset, format!("    // {name}\n    digest!(\"{}\"),", proc_info.digest))
        })
        .collect();

    let proc_count = generated_procs.len();
    let generated_procs: String = generated_procs.into_iter().enumerate().map(|(index, (offset, txt))| {
        if index != offset {
            panic!("Offset constants in the file `{offsets_filename:?}` are not contiguous (missing offset: {index})");
        }

        txt
    }).collect::<Vec<_>>().join("\n");

    fs::write(
        KERNEL_V0_RS_FILE,
        format!(
            r#"//! This file is generated by build.rs, do not modify

use miden_objects::{{digest, Digest}};

// KERNEL V0 PROCEDURES
// ================================================================================================

/// Hashes of all dynamically executed procedures from the kernel 0.
pub const KERNEL0_PROCEDURES: [Digest; {proc_count}] = [
{generated_procs}
];
"#,
        ),
    )
    .into_diagnostic()
}

fn parse_proc_offsets(filename: impl AsRef<Path>) -> Result<BTreeMap<String, usize>> {
    let regex: Regex = Regex::new(r"^const\.(?P<name>\w+)_OFFSET\s*=\s*(?P<offset>\d+)").unwrap();
    let mut result = BTreeMap::new();
    for line in fs::read_to_string(filename).into_diagnostic()?.lines() {
        if let Some(captures) = regex.captures(line) {
            result.insert(
                captures["name"].to_string().to_lowercase(),
                captures["offset"].parse().into_diagnostic()?,
            );
        }
    }

    Ok(result)
}

// COMPILE MIDEN LIB
// ================================================================================================

/// Reads the MASM files from "{source_dir}/miden" directory, compiles them into a Miden assembly
/// library, saves the library into "{target_dir}/miden.masl", and returns the compiled library.
fn compile_miden_lib(
    source_dir: &Path,
    target_dir: &Path,
    mut assembler: Assembler,
) -> Result<Library> {
    let source_dir = source_dir.join(ASM_MIDEN_DIR);
    let shared_path = Path::new(ASM_DIR).join(SHARED_DIR);

    let miden_namespace = "miden".parse::<LibraryNamespace>().expect("invalid base namespace");
    // add the shared modules to the kernel lib under the miden::util namespace
    assembler.add_modules_from_dir(miden_namespace.clone(), &shared_path)?;

    let miden_lib = Library::from_dir(source_dir, miden_namespace, assembler)?;

    let output_file = target_dir.join("miden").with_extension(Library::LIBRARY_EXTENSION);
    miden_lib.write_to_file(output_file).into_diagnostic()?;

    Ok(miden_lib)
}

// COMPILE EXECUTABLE MODULES
// ================================================================================================

/// Reads all MASM files from the "{source_dir}", complies each file individually into a MASB
/// file, and stores the compiled files into the "{target_dir}".
///
/// The source files are expected to contain executable programs.
fn compile_note_scripts(
    source_dir: &Path,
    target_dir: &Path,
    mut assembler: Assembler,
) -> Result<()> {
    if let Err(e) = fs::create_dir_all(target_dir) {
        println!("Failed to create note_scripts directory: {}", e);
    }

    // Add utils.masm as a library to the assembler
    let utils_file_path = source_dir.join("utils.masm");
    if utils_file_path.exists() {
        let utils_namespace =
            "note_scripts::utils".parse::<LibraryNamespace>().expect("invalid namespace");
        let utils_source = fs::read_to_string(&utils_file_path).into_diagnostic()?;

        assembler.add_module(NamedSource::new(utils_namespace, utils_source.clone()))?;
    }

    for masm_file_path in get_masm_files(source_dir).unwrap() {
        // Skip utils.masm since it was added as a library
        if masm_file_path.file_name().unwrap().to_str().unwrap() == "utils.masm" {
            continue;
        }

        // read the MASM file, parse it, and serialize the parsed AST to bytes
        let code = assembler.clone().assemble_program(masm_file_path.clone())?;

        let bytes = code.to_bytes();

        // TODO: get rid of unwraps
        let masb_file_name = masm_file_path.file_name().unwrap().to_str().unwrap();
        let mut masb_file_path = target_dir.join(masb_file_name);

        // write the binary MASB to the output dir
        masb_file_path.set_extension("masb");
        fs::write(masb_file_path, bytes).unwrap();
    }
    Ok(())
}

// COMPILE ACCOUNT COMPONENTS
// ================================================================================================

/// Compiles the account components in `source_dir` into MASL libraries and stores the compiled
/// files in `target_dir`.
fn compile_account_components(
    source_dir: &Path,
    target_dir: &Path,
    assembler: Assembler,
) -> Result<()> {
    if !target_dir.exists() {
        fs::create_dir_all(target_dir).unwrap();
    }

    for masm_file_path in get_masm_files(source_dir).unwrap() {
        let component_name = masm_file_path
            .file_stem()
            .expect("masm file should have a file stem")
            .to_str()
            .expect("file stem should be valid UTF-8")
            .to_owned();

        // Read the source code to string instead of passing it to assemble_library directly since
        // that would attempt to interpret the path as a LibraryPath which would fail.
        let component_source_code = fs::read_to_string(masm_file_path)
            .expect("reading the component's MASM source code should succeed");

        let component_library = assembler
            .clone()
            .assemble_library([component_source_code])
            .expect("library assembly should succeed");
        let component_file_path =
            target_dir.join(component_name).with_extension(Library::LIBRARY_EXTENSION);
        component_library.write_to_file(component_file_path).into_diagnostic()?;
    }

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

/// Returns a new [Assembler] loaded with miden-stdlib and the specified kernel, if provided.
///
/// The returned assembler will be in the `debug` mode if the `with-debug-info` feature is enabled.
fn build_assembler(kernel: Option<KernelLibrary>) -> Result<Assembler> {
    kernel
        .map(|kernel| Assembler::with_kernel(Arc::new(DefaultSourceManager::default()), kernel))
        .unwrap_or_default()
        .with_debug_mode(cfg!(feature = "with-debug-info"))
        .with_library(miden_stdlib::StdLibrary::default())
}

/// Recursively copies `src` into `dst`.
///
/// This function will overwrite the existing files if re-executed.
fn copy_directory<T: AsRef<Path>, R: AsRef<Path>>(src: T, dst: R) {
    let mut prefix = src.as_ref().canonicalize().unwrap();
    // keep all the files inside the `asm` folder
    prefix.pop();

    let target_dir = dst.as_ref().join(ASM_DIR);
    if !target_dir.exists() {
        fs::create_dir_all(target_dir).unwrap();
    }

    let dst = dst.as_ref();
    let mut todo = vec![src.as_ref().to_path_buf()];

    while let Some(goal) = todo.pop() {
        for entry in fs::read_dir(goal).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                let src_dir = path.canonicalize().unwrap();
                let dst_dir = dst.join(src_dir.strip_prefix(&prefix).unwrap());
                if !dst_dir.exists() {
                    fs::create_dir_all(&dst_dir).unwrap();
                }
                todo.push(src_dir);
            } else {
                let dst_file = dst.join(path.strip_prefix(&prefix).unwrap());
                fs::copy(&path, dst_file).unwrap();
            }
        }
    }
}

/// Returns a vector with paths to all MASM files in the specified directory.
///
/// All non-MASM files are skipped.
fn get_masm_files<P: AsRef<Path>>(dir_path: P) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let path = dir_path.as_ref();
    if path.is_dir() {
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(file) => {
                            let file_path = file.path();
                            if is_masm_file(&file_path)? {
                                files.push(file_path);
                            }
                        },
                        Err(e) => println!("Error reading directory entry: {}", e),
                    }
                }
            },
            Err(e) => println!("Error reading directory: {}", e),
        }
    } else {
        println!("cargo:rerun-The specified path is not a directory.");
    }

    Ok(files)
}

/// Returns true if the provided path resolves to a file with `.masm` extension.
///
/// # Errors
/// Returns an error if the path could not be converted to a UTF-8 string.
fn is_masm_file(path: &Path) -> io::Result<bool> {
    if let Some(extension) = path.extension() {
        let extension = extension
            .to_str()
            .ok_or_else(|| io::Error::other("invalid UTF-8 filename"))?
            .to_lowercase();
        Ok(extension == "masm")
    } else {
        Ok(false)
    }
}

// ERROR CONSTANTS FILE GENERATION
// ================================================================================================

/// Reads all MASM files from the `asm_source_dir` and extracts its error constants and their
/// associated error message and generates a Rust file for each category of errors.
/// For example:
///
/// ```text
/// const.ERR_PROLOGUE_NEW_ACCOUNT_VAULT_MUST_BE_EMPTY="new account must have an empty vault"
/// ```
///
/// would generate a Rust file for transaction kernel errors (since the error belongs to that
/// category, identified by the category extracted from `ERR_<CATEGORY>`) with - roughly - the
/// following content:
///
/// ```rust
/// pub const ERR_PROLOGUE_NEW_ACCOUNT_VAULT_MUST_BE_EMPTY: MasmError =
///     MasmError::from_static_str("new account must have an empty vault");
/// ```
///
/// and add the constant to the error constants array.
///
/// The function ensures that a constant is not defined twice, except if their error message is the
/// same. This can happen across multiple files.
///
/// Because the error files will be written to ./src/errors, this should be a no-op if ./src is
/// read-only. To enable writing to ./src, set the `BUILD_GENERATED_FILES_IN_SRC` environment
/// variable.
fn generate_error_constants(asm_source_dir: &Path) -> Result<()> {
    if !BUILD_GENERATED_FILES_IN_SRC {
        return Ok(());
    }

    let categories =
        extract_all_masm_errors(asm_source_dir).context("failed to extract all masm errors")?;

    for (category, errors) in categories {
        // Generate the errors file.
        let error_file_content = generate_error_file_content(category, errors)?;
        std::fs::write(category.error_file_name(), error_file_content).into_diagnostic()?;
    }

    Ok(())
}

/// Extract all masm errors from the given path and returns a map by error category.
fn extract_all_masm_errors(asm_source_dir: &Path) -> Result<ErrorCategoryMap> {
    // We use a BTree here to order the errors by their categories which is the first part after the
    // ERR_ prefix and to allow for the same error to be defined multiple times in different files
    // (as long as the constant name and error messages match).
    let mut errors = BTreeMap::new();

    // Walk all files of the kernel source directory.
    for entry in WalkDir::new(asm_source_dir) {
        let entry = entry.into_diagnostic()?;
        if !is_masm_file(entry.path()).into_diagnostic()? {
            continue;
        }
        let file_contents = std::fs::read_to_string(entry.path()).into_diagnostic()?;
        extract_masm_errors(&mut errors, &file_contents)?;
    }

    let mut category_map: BTreeMap<ErrorCategory, Vec<NamedError>> = BTreeMap::new();

    for (error_name, error) in errors.into_iter() {
        let category = ErrorCategory::match_category(&error_name)?;

        let named_error = NamedError { name: error_name, message: error.message };

        category_map.entry(category).or_default().push(named_error);
    }

    Ok(category_map)
}

/// Extracts the errors from a single masm file and inserts them into the provided map.
fn extract_masm_errors(
    errors: &mut BTreeMap<ErrorName, ExtractedError>,
    file_contents: &str,
) -> Result<()> {
    let regex = Regex::new(r#"const\.ERR_(?<name>.*)="(?<message>.*)""#).unwrap();

    for capture in regex.captures_iter(file_contents) {
        let error_name = capture
            .name("name")
            .expect("error name should be captured")
            .as_str()
            .trim()
            .to_owned();
        let error_message = capture
            .name("message")
            .expect("error code should be captured")
            .as_str()
            .trim()
            .to_owned();

        if let Some(ExtractedError { message: existing_error_message, .. }) =
            errors.get(&error_name)
        {
            if existing_error_message != &error_message {
                return Err(Report::msg(format!(
                    "Transaction kernel error constant ERR_{error_name} is already defined elsewhere but its error message is different"
                )));
            }
        }

        // Enforce the "no trailing punctuation" rule from the Rust error guidelines on MASM errors.
        if error_message.ends_with(".") {
            return Err(Report::msg(format!(
                "Error messages should not end with a period: `ERR_{error_name}: {error_message}`"
            )));
        }

        errors.insert(error_name, ExtractedError { message: error_message });
    }

    Ok(())
}

fn is_new_error_category<'a>(last_error: &mut Option<&'a str>, current_error: &'a str) -> bool {
    let is_new = match last_error {
        Some(last_err) => {
            let last_category =
                last_err.split("_").next().expect("there should be at least one entry");
            let new_category =
                current_error.split("_").next().expect("there should be at least one entry");
            last_category != new_category
        },
        None => false,
    };

    last_error.replace(current_error);

    is_new
}

/// Generates the content of an error file for the given category and the set of errors.
fn generate_error_file_content(category: ErrorCategory, errors: Vec<NamedError>) -> Result<String> {
    let mut output = String::new();

    writeln!(output, "use crate::errors::MasmError;\n").unwrap();

    writeln!(
        output,
        "// This file is generated by build.rs, do not modify manually.
// It is generated by extracting errors from the masm files in the `miden-lib/asm` directory.
//
// To add a new error, define a constant in masm of the pattern `const.ERR_<CATEGORY>_...`.
// Try to fit the error into a pre-existing category if possible (e.g. Account, Prologue,
// Non-Fungible-Asset, ...).
"
    )
    .unwrap();

    writeln!(
        output,
        "// {}
// ================================================================================================
",
        category.array_name().replace("_", " ")
    )
    .unwrap();

    let mut last_error = None;
    for named_error in errors.iter() {
        let NamedError { name, message } = named_error;

        // Group errors into blocks separate by newlines.
        if is_new_error_category(&mut last_error, name) {
            writeln!(output).into_diagnostic()?;
        }

        writeln!(output, "/// Error Message: \"{message}\"").into_diagnostic()?;
        writeln!(
            output,
            r#"pub const ERR_{name}: MasmError = MasmError::from_static_str("{message}");"#
        )
        .into_diagnostic()?;
    }

    Ok(output)
}

type ErrorName = String;

#[derive(Debug, Clone)]
struct ExtractedError {
    message: String,
}

#[derive(Debug, Clone)]
struct NamedError {
    name: ErrorName,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ErrorCategory {
    TxKernel,
    NoteScript,
}

impl ErrorCategory {
    pub const fn error_file_name(&self) -> &'static str {
        match self {
            ErrorCategory::TxKernel => TX_KERNEL_ERRORS_FILE,
            ErrorCategory::NoteScript => NOTE_SCRIPT_ERRORS_FILE,
        }
    }

    pub const fn array_name(&self) -> &'static str {
        match self {
            ErrorCategory::TxKernel => TX_KERNEL_ERRORS_ARRAY_NAME,
            ErrorCategory::NoteScript => NOTE_SCRIPT_ERRORS_ARRAY_NAME,
        }
    }

    pub fn match_category(error_name: &ErrorName) -> Result<Self> {
        for kernel_category in TX_KERNEL_ERROR_CATEGORIES {
            if error_name.starts_with(kernel_category.category_name()) {
                return Ok(ErrorCategory::TxKernel);
            }
        }

        // If the error is not a tx kernel error, consider it a note script error.
        Ok(ErrorCategory::NoteScript)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TxKernelErrorCategory {
    Kernel,
    Prologue,
    Epilogue,
    Tx,
    Note,
    Account,
    ForeignAccount,
    Faucet,
    FungibleAsset,
    NonFungibleAsset,
    Vault,
}

impl TxKernelErrorCategory {
    pub const fn category_name(&self) -> &'static str {
        match self {
            TxKernelErrorCategory::Kernel => "KERNEL",
            TxKernelErrorCategory::Prologue => "PROLOGUE",
            TxKernelErrorCategory::Epilogue => "EPILOGUE",
            TxKernelErrorCategory::Tx => "TX",
            TxKernelErrorCategory::Note => "NOTE",
            TxKernelErrorCategory::Account => "ACCOUNT",
            TxKernelErrorCategory::ForeignAccount => "FOREIGN_ACCOUNT",
            TxKernelErrorCategory::Faucet => "FAUCET",
            TxKernelErrorCategory::FungibleAsset => "FUNGIBLE_ASSET",
            TxKernelErrorCategory::NonFungibleAsset => "NON_FUNGIBLE_ASSET",
            TxKernelErrorCategory::Vault => "VAULT",
        }
    }
}
