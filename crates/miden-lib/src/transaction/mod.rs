use alloc::{string::ToString, sync::Arc, vec::Vec};

use miden_objects::{
    EMPTY_WORD, Felt, Hasher, TransactionOutputError, Word,
    account::AccountId,
    assembly::{Assembler, DefaultSourceManager, KernelLibrary, SourceManager},
    block::BlockNumber,
    transaction::{
        OutputNote, OutputNotes, TransactionArgs, TransactionInputs, TransactionOutputs,
    },
    utils::{serde::Deserializable, sync::LazyLock},
    vm::{AdviceInputs, Program, ProgramInfo, StackInputs, StackOutputs},
};
use miden_stdlib::StdLibrary;
use outputs::EXPIRATION_BLOCK_ELEMENT_IDX;

use super::MidenLib;

pub mod memory;

mod events;
pub use events::TransactionEvent;

mod inputs;
pub use inputs::TransactionAdviceInputs;

mod outputs;
pub use outputs::{
    ACCOUNT_UPDATE_COMMITMENT_WORD_IDX, OUTPUT_NOTES_COMMITMENT_WORD_IDX,
    parse_final_account_header,
};

pub use crate::errors::{
    TransactionEventError, TransactionKernelError, TransactionTraceParsingError,
};

mod procedures;

// CONSTANTS
// ================================================================================================

// Initialize the kernel library only once
static KERNEL_LIB: LazyLock<KernelLibrary> = LazyLock::new(|| {
    let kernel_lib_bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/kernels/tx_kernel.masl"));
    KernelLibrary::read_from_bytes(kernel_lib_bytes)
        .expect("failed to deserialize transaction kernel library")
});

// Initialize the kernel main program only once
static KERNEL_MAIN: LazyLock<Program> = LazyLock::new(|| {
    let kernel_main_bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/kernels/tx_kernel.masb"));
    Program::read_from_bytes(kernel_main_bytes)
        .expect("failed to deserialize transaction kernel runtime")
});

// Initialize the transaction script executor program only once
static TX_SCRIPT_MAIN: LazyLock<Program> = LazyLock::new(|| {
    let tx_script_main_bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/kernels/tx_script_main.masb"));
    Program::read_from_bytes(tx_script_main_bytes)
        .expect("failed to deserialize tx script executor runtime")
});

// TRANSACTION KERNEL
// ================================================================================================

pub struct TransactionKernel;

impl TransactionKernel {
    // KERNEL SOURCE CODE
    // --------------------------------------------------------------------------------------------

    /// Returns a library with the transaction kernel system procedures.
    ///
    /// # Panics
    /// Panics if the transaction kernel source is not well-formed.
    pub fn kernel() -> KernelLibrary {
        KERNEL_LIB.clone()
    }

    /// Returns an AST of the transaction kernel executable program.
    ///
    /// # Panics
    /// Panics if the transaction kernel source is not well-formed.
    pub fn main() -> Program {
        KERNEL_MAIN.clone()
    }

    /// Returns an AST of the transaction script executor program.
    ///
    /// # Panics
    /// Panics if the transaction kernel source is not well-formed.
    pub fn tx_script_main() -> Program {
        TX_SCRIPT_MAIN.clone()
    }

    /// Returns [ProgramInfo] for the transaction kernel executable program.
    ///
    /// # Panics
    /// Panics if the transaction kernel source is not well-formed.
    pub fn program_info() -> ProgramInfo {
        // TODO: make static
        let program_hash = Self::main().hash();
        let kernel = Self::kernel().kernel().clone();

        ProgramInfo::new(program_hash, kernel)
    }

    /// Transforms the provided [TransactionInputs] and [TransactionArgs] into stack and advice
    /// inputs needed to execute a transaction kernel for a specific transaction.
    ///
    /// If `init_advice_inputs` is provided, they will be included in the returned advice inputs.
    pub fn prepare_inputs(
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
        init_advice_inputs: Option<AdviceInputs>,
    ) -> (StackInputs, TransactionAdviceInputs) {
        let account = tx_inputs.account();

        let stack_inputs = TransactionKernel::build_input_stack(
            account.id(),
            account.init_commitment(),
            tx_inputs.input_notes().commitment(),
            tx_inputs.block_header().commitment(),
            tx_inputs.block_header().block_num(),
        );

        let mut tx_advice_inputs = TransactionAdviceInputs::new(tx_inputs, tx_args);
        if let Some(init_advice_inputs) = init_advice_inputs {
            tx_advice_inputs.extend(init_advice_inputs);
        }

        (stack_inputs, tx_advice_inputs)
    }

    // ASSEMBLER CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new Miden assembler instantiated with the transaction kernel and loaded with the
    /// Miden stdlib as well as with miden-lib.
    pub fn assembler() -> Assembler {
        let source_manager: Arc<dyn SourceManager + Send + Sync> =
            Arc::new(DefaultSourceManager::default());

        #[cfg(all(any(feature = "testing", test), feature = "std"))]
        source_manager_ext::load_masm_source_files(&source_manager);

        Assembler::with_kernel(source_manager, Self::kernel())
            .with_dynamic_library(StdLibrary::default())
            .expect("failed to load std-lib")
            .with_dynamic_library(MidenLib::default())
            .expect("failed to load miden-lib")
    }

    // STACK INPUTS / OUTPUTS
    // --------------------------------------------------------------------------------------------

    /// Returns the stack with the public inputs required by the transaction kernel.
    ///
    /// The initial stack is defined:
    ///
    /// ```text
    /// [
    ///     BLOCK_COMMITMENT,
    ///     INITIAL_ACCOUNT_COMMITMENT,
    ///     INPUT_NOTES_COMMITMENT,
    ///     account_id_prefix, account_id_suffix, block_num
    /// ]
    /// ```
    ///
    /// Where:
    /// - BLOCK_COMMITMENT is the commitment to the reference block of the transaction.
    /// - block_num is the reference block number.
    /// - account_id_{prefix,suffix} are the prefix and suffix felts of the account that the
    ///   transaction is being executed against.
    /// - INITIAL_ACCOUNT_COMMITMENT is the account state prior to the transaction, EMPTY_WORD for
    ///   new accounts.
    /// - INPUT_NOTES_COMMITMENT, see `transaction::api::get_input_notes_commitment`.
    pub fn build_input_stack(
        account_id: AccountId,
        init_account_commitment: Word,
        input_notes_commitment: Word,
        block_commitment: Word,
        block_num: BlockNumber,
    ) -> StackInputs {
        // Note: Must be kept in sync with the transaction's kernel prepare_transaction procedure
        let mut inputs: Vec<Felt> = Vec::with_capacity(14);
        inputs.push(Felt::from(block_num));
        inputs.push(account_id.suffix());
        inputs.push(account_id.prefix().as_felt());
        inputs.extend(input_notes_commitment);
        inputs.extend_from_slice(init_account_commitment.as_elements());
        inputs.extend_from_slice(block_commitment.as_elements());
        StackInputs::new(inputs)
            .map_err(|e| e.to_string())
            .expect("Invalid stack input")
    }

    /// Builds the stack for expected transaction execution outputs.
    /// The transaction kernel's output stack is formed like so:
    ///
    /// ```text
    /// [
    ///     OUTPUT_NOTES_COMMITMENT,
    ///     ACCOUNT_UPDATE_COMMITMENT,
    ///     expiration_block_num,
    /// ]
    /// ```
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is a commitment to the output notes.
    /// - ACCOUNT_UPDATE_COMMITMENT is the hash of the the final account commitment and account
    ///   delta commitment.
    /// - expiration_block_num is the block number at which the transaction will expire.
    pub fn build_output_stack(
        final_account_commitment: Word,
        account_delta_commitment: Word,
        output_notes_commitment: Word,
        expiration_block_num: BlockNumber,
    ) -> StackOutputs {
        let account_update_commitment =
            Hasher::merge(&[final_account_commitment, account_delta_commitment]);
        let mut outputs: Vec<Felt> = Vec::with_capacity(9);
        outputs.push(Felt::from(expiration_block_num));
        outputs.extend(account_update_commitment);
        outputs.extend(output_notes_commitment);
        outputs.reverse();
        StackOutputs::new(outputs)
            .map_err(|e| e.to_string())
            .expect("Invalid stack output")
    }

    /// Extracts transaction output data from the provided stack outputs.
    ///
    /// The data on the stack is expected to be arranged as follows:
    ///
    /// Stack: [OUTPUT_NOTES_COMMITMENT, ACCOUNT_UPDATE_COMMITMENT, tx_expiration_block_num]
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is the commitment of the output notes.
    /// - ACCOUNT_UPDATE_COMMITMENT is the hash of the the final account commitment and account
    ///   delta commitment.
    /// - tx_expiration_block_num is the block height at which the transaction will become expired,
    ///   defined by the sum of the execution block ref and the transaction's block expiration delta
    ///   (if set during transaction execution).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Indices 9..16 on the stack are not zeroes.
    /// - Overflow addresses are not empty.
    pub fn parse_output_stack(
        stack: &StackOutputs,
    ) -> Result<(Word, Word, BlockNumber), TransactionOutputError> {
        let output_notes_commitment = stack
            .get_stack_word(OUTPUT_NOTES_COMMITMENT_WORD_IDX * 4)
            .expect("output_notes_commitment (first word) missing");

        let account_update_commitment = stack
            .get_stack_word(ACCOUNT_UPDATE_COMMITMENT_WORD_IDX * 4)
            .expect("account_update_commitment (second word) missing");

        let expiration_block_num = stack
            .get_stack_item(EXPIRATION_BLOCK_ELEMENT_IDX)
            .expect("element on index 8 missing");

        let expiration_block_num = u32::try_from(expiration_block_num.as_int())
            .map_err(|_| {
                TransactionOutputError::OutputStackInvalid(
                    "expiration block number should be smaller than u32::MAX".into(),
                )
            })?
            .into();

        // Make sure that indices 9, 10 and 11 are zeroes (i.e. the third word without the
        // expiration block number).
        if stack.get_stack_word(9).expect("third word missing").as_elements()[..3]
            != Word::empty().as_elements()[..3]
        {
            return Err(TransactionOutputError::OutputStackInvalid(
                "indices 9, 10 and 11 on the output stack should be ZERO".into(),
            ));
        }

        if stack.get_stack_word(12).expect("fourth word missing") != EMPTY_WORD {
            return Err(TransactionOutputError::OutputStackInvalid(
                "fourth word on output stack should consist only of ZEROs".into(),
            ));
        }

        Ok((output_notes_commitment, account_update_commitment, expiration_block_num))
    }

    // TRANSACTION OUTPUT PARSER
    // --------------------------------------------------------------------------------------------

    /// Returns [TransactionOutputs] constructed from the provided output stack and advice map.
    ///
    /// The output stack is expected to be arrange as follows:
    ///
    /// Stack: [OUTPUT_NOTES_COMMITMENT, ACCOUNT_UPDATE_COMMITMENT, tx_expiration_block_num]
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is the commitment of the output notes.
    /// - ACCOUNT_UPDATE_COMMITMENT is the hash of the final account commitment and the account
    ///   delta commitment of the account that the transaction is being executed against.
    /// - tx_expiration_block_num is the block height at which the transaction will become expired,
    ///   defined by the sum of the execution block ref and the transaction's block expiration delta
    ///   (if set during transaction execution).
    ///
    /// The actual data describing the new account state and output notes is expected to be located
    /// in the provided advice map under keys `OUTPUT_NOTES_COMMITMENT` and
    /// `ACCOUNT_UPDATE_COMMITMENT`, where the final data for the account state is located under
    /// `FINAL_ACCOUNT_COMMITMENT`.
    pub fn from_transaction_parts(
        stack: &StackOutputs,
        advice_inputs: &AdviceInputs,
        output_notes: Vec<OutputNote>,
    ) -> Result<TransactionOutputs, TransactionOutputError> {
        let (output_notes_commitment, account_update_commitment, expiration_block_num) =
            Self::parse_output_stack(stack)?;

        let (final_account_commitment, account_delta_commitment) =
            Self::parse_account_update_commitment(account_update_commitment, advice_inputs)?;

        // parse final account state
        let final_account_data = advice_inputs
            .map
            .get(&final_account_commitment)
            .ok_or(TransactionOutputError::FinalAccountCommitmentMissingInAdviceMap)?;

        let account = parse_final_account_header(final_account_data)
            .map_err(TransactionOutputError::FinalAccountHeaderParseFailure)?;

        // validate output notes
        let output_notes = OutputNotes::new(output_notes)?;
        if output_notes_commitment != output_notes.commitment() {
            return Err(TransactionOutputError::OutputNotesCommitmentInconsistent {
                actual: output_notes.commitment(),
                expected: output_notes_commitment,
            });
        }

        Ok(TransactionOutputs {
            account,
            account_delta_commitment,
            output_notes,
            expiration_block_num,
        })
    }

    /// Returns the final account commitment and account delta commitment extracted from the account
    /// update commitment.
    fn parse_account_update_commitment(
        account_update_commitment: Word,
        advice_inputs: &AdviceInputs,
    ) -> Result<(Word, Word), TransactionOutputError> {
        let account_update_data =
            advice_inputs.map.get(&account_update_commitment).ok_or_else(|| {
                TransactionOutputError::AccountUpdateCommitment(
                    "failed to find ACCOUNT_UPDATE_COMMITMENT in advice map".into(),
                )
            })?;

        if account_update_data.len() != 8 {
            return Err(TransactionOutputError::AccountUpdateCommitment(
                "expected account update commitment advice map entry to contain exactly 8 elements"
                    .into(),
            ));
        }

        // SAFETY: We just asserted that the data is of length 8 so slicing the data into two words
        // is fine.
        let final_account_commitment = Word::from(
            <[Felt; 4]>::try_from(&account_update_data[0..4])
                .expect("we should have sliced off exactly four elements"),
        );
        let account_delta_commitment = Word::from(
            <[Felt; 4]>::try_from(&account_update_data[4..8])
                .expect("we should have sliced off exactly four elements"),
        );

        let computed_account_update_commitment =
            Hasher::merge(&[final_account_commitment, account_delta_commitment]);

        if computed_account_update_commitment != account_update_commitment {
            let err_message = format!(
                "transaction outputs account update commitment {account_update_commitment} but commitment computed from its advice map entries was {computed_account_update_commitment}"
            );
            return Err(TransactionOutputError::AccountUpdateCommitment(err_message.into()));
        }

        Ok((final_account_commitment, account_delta_commitment))
    }
}

#[cfg(any(feature = "testing", test))]
impl TransactionKernel {
    const KERNEL_TESTING_LIB_BYTES: &'static [u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/kernels/kernel_library.masl"));

    pub fn kernel_as_library() -> miden_objects::assembly::Library {
        miden_objects::assembly::Library::read_from_bytes(Self::KERNEL_TESTING_LIB_BYTES)
            .expect("failed to deserialize transaction kernel library")
    }

    /// Contains code to get an instance of the [Assembler] that should be used in tests.
    ///
    /// This assembler is similar to the assembler used to assemble the kernel and transactions,
    /// with the difference that it also includes an extra library on the namespace of `kernel`.
    /// The `kernel` library is added separately because even though the library (`api.masm`) and
    /// the kernel binary (`main.masm`) include this code, it is not exposed explicitly. By adding
    /// it separately, we can expose procedures from `/lib` and test them individually.
    pub fn testing_assembler() -> Assembler {
        let source_manager: Arc<dyn SourceManager + Send + Sync> =
            Arc::new(DefaultSourceManager::default());
        let kernel_library = Self::kernel_as_library();

        #[cfg(all(any(feature = "testing", test), feature = "std"))]
        source_manager_ext::load_masm_source_files(&source_manager);

        Assembler::with_kernel(source_manager, Self::kernel())
            .with_dynamic_library(StdLibrary::default())
            .expect("failed to load std-lib")
            .with_dynamic_library(MidenLib::default())
            .expect("failed to load miden-lib")
            .with_dynamic_library(kernel_library)
            .expect("failed to load kernel library (/lib)")
            .with_debug_mode(true)
    }

    /// Returns the testing assembler, and additionally contains the library for
    /// [AccountCode::mock_library](miden_objects::account::AccountCode::mock_library), which is a
    /// mock wallet used in tests.
    pub fn testing_assembler_with_mock_account() -> Assembler {
        let assembler = Self::testing_assembler().with_debug_mode(true);
        let library = miden_objects::account::AccountCode::mock_library(assembler.clone());

        assembler
            .with_dynamic_library(library)
            .expect("failed to add mock account code")
    }
}

#[cfg(all(any(feature = "testing", test), feature = "std"))]
mod source_manager_ext {
    use std::{
        fs, io,
        path::{Path, PathBuf},
        sync::Arc,
        vec::Vec,
    };

    use miden_objects::assembly::{SourceManager, debuginfo::SourceManagerExt};

    /// Loads all files with a .masm extension in the `asm` directory into the provided source
    /// manager.
    ///
    /// This source manager is passed to the [`super::TransactionKernel::assembler`] from which it
    /// can be passed on to the VM processor. If an error occurs, the sources can be used to provide
    /// a pointer to the failed location.
    pub fn load_masm_source_files(source_manager: &Arc<dyn SourceManager + Send + Sync>) {
        if let Err(err) = load(source_manager) {
            // Stringifying the error is not ideal (we may loose some source errors) but this
            // should never really error anyway.
            std::eprintln!("failed to load MASM sources into source manager: {err}");
        }
    }

    /// Implements the logic of the above function with error handling.
    fn load(source_manager: &Arc<dyn SourceManager + Send + Sync>) -> io::Result<()> {
        for file in get_masm_files(concat!(env!("OUT_DIR"), "/asm"))? {
            source_manager.load_file(&file).map_err(io::Error::other)?;
        }

        Ok(())
    }

    /// Returns a vector with paths to all MASM files in the specified directory and recursive
    /// directories.
    ///
    /// All non-MASM files are skipped.
    fn get_masm_files<P: AsRef<Path>>(dir_path: P) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        match fs::read_dir(dir_path) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => {
                            let entry_path = entry.path();
                            if entry_path.is_dir() {
                                files.extend(get_masm_files(entry_path)?);
                            } else if entry_path
                                .extension()
                                .map(|ext| ext == "masm")
                                .unwrap_or(false)
                            {
                                files.push(entry_path);
                            }
                        },
                        Err(e) => {
                            return Err(io::Error::other(format!(
                                "error reading directory entry: {e}",
                            )));
                        },
                    }
                }
            },
            Err(e) => {
                return Err(io::Error::other(format!("error reading directory: {e}")));
            },
        }

        Ok(files)
    }
}
