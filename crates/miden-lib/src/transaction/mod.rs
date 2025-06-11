use alloc::{string::ToString, sync::Arc, vec::Vec};

use miden_objects::{
    Digest, EMPTY_WORD, Felt, TransactionInputError, TransactionOutputError,
    account::{AccountCode, AccountId},
    assembly::{Assembler, DefaultSourceManager, KernelLibrary},
    block::BlockNumber,
    transaction::{
        OutputNote, OutputNotes, TransactionArgs, TransactionInputs, TransactionOutputs,
    },
    utils::{serde::Deserializable, sync::LazyLock},
    vm::{AdviceInputs, AdviceMap, Program, ProgramInfo, StackInputs, StackOutputs},
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
    FINAL_ACCOUNT_COMMITMENT_WORD_IDX, OUTPUT_NOTES_COMMITMENT_WORD_IDX, parse_final_account_header,
};

mod errors;
pub use errors::{TransactionEventError, TransactionKernelError, TransactionTraceParsingError};

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
    ) -> Result<(StackInputs, TransactionAdviceInputs), TransactionInputError> {
        let account = tx_inputs.account();

        let stack_inputs = TransactionKernel::build_input_stack(
            account.id(),
            account.init_commitment(),
            tx_inputs.input_notes().commitment(),
            tx_inputs.block_header().commitment(),
            tx_inputs.block_header().block_num(),
        );

        let mut tx_advice_inputs = TransactionAdviceInputs::new(tx_inputs, tx_args)?;
        if let Some(init_advice_inputs) = init_advice_inputs {
            tx_advice_inputs.extend(init_advice_inputs);
        }

        Ok((stack_inputs, tx_advice_inputs))
    }

    // ASSEMBLER CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new Miden assembler instantiated with the transaction kernel and loaded with the
    /// Miden stdlib as well as with miden-lib.
    pub fn assembler() -> Assembler {
        let source_manager = Arc::new(DefaultSourceManager::default());
        Assembler::with_kernel(source_manager, Self::kernel())
            .with_library(StdLibrary::default())
            .expect("failed to load std-lib")
            .with_library(MidenLib::default())
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
        init_account_commitment: Digest,
        input_notes_commitment: Digest,
        block_commitment: Digest,
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
    ///     expiration_block_num,
    ///     OUTPUT_NOTES_COMMITMENT,
    ///     FINAL_ACCOUNT_COMMITMENT,
    /// ]
    /// ```
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is a commitment to the output notes.
    /// - FINAL_ACCOUNT_COMMITMENT is a hash of the account's final state.
    /// - expiration_block_num is the block number at which the transaction will expire.
    pub fn build_output_stack(
        final_account_commitment: Digest,
        output_notes_commitment: Digest,
        expiration_block_num: BlockNumber,
    ) -> StackOutputs {
        let mut outputs: Vec<Felt> = Vec::with_capacity(9);
        outputs.push(Felt::from(expiration_block_num));
        outputs.extend(final_account_commitment);
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
    /// Stack: [OUTPUT_NOTES_COMMITMENT, FINAL_ACCOUNT_COMMITMENT, tx_expiration_block_num]
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is the commitment of the output notes.
    /// - FINAL_ACCOUNT_COMMITMENT is the final account commitment of the account that the
    ///   transaction is being executed against.
    /// - tx_expiration_block_num is the block height at which the transaction will become expired,
    ///   defined by the sum of the execution block ref and the transaction's block expiration delta
    ///   (if set during transaction execution).
    ///
    /// # Errors
    /// Returns an error if:
    /// - Words 3 and 4 on the stack are not 0.
    /// - Overflow addresses are not empty.
    pub fn parse_output_stack(
        stack: &StackOutputs,
    ) -> Result<(Digest, Digest, BlockNumber), TransactionOutputError> {
        let output_notes_commitment = stack
            .get_stack_word(OUTPUT_NOTES_COMMITMENT_WORD_IDX * 4)
            .expect("first word missing")
            .into();

        let final_account_commitment = stack
            .get_stack_word(FINAL_ACCOUNT_COMMITMENT_WORD_IDX * 4)
            .expect("second word missing")
            .into();

        let expiration_block_num = stack
            .get_stack_item(EXPIRATION_BLOCK_ELEMENT_IDX)
            .expect("element on index 8 missing");

        let expiration_block_num = u32::try_from(expiration_block_num.as_int())
            .map_err(|_| {
                TransactionOutputError::OutputStackInvalid(
                    "Expiration block number should be smaller than u32::MAX".into(),
                )
            })?
            .into();

        if stack.get_stack_word(12).expect("fourth word missing") != EMPTY_WORD {
            return Err(TransactionOutputError::OutputStackInvalid(
                "Fourth word on output stack should consist only of ZEROs".into(),
            ));
        }

        Ok((final_account_commitment, output_notes_commitment, expiration_block_num))
    }

    // TRANSACTION OUTPUT PARSER
    // --------------------------------------------------------------------------------------------

    /// Returns [TransactionOutputs] constructed from the provided output stack and advice map.
    ///
    /// The output stack is expected to be arrange as follows:
    ///
    /// Stack: [OUTPUT_NOTES_COMMITMENT, FINAL_ACCOUNT_COMMITMENT, tx_expiration_block_num]
    ///
    /// Where:
    /// - OUTPUT_NOTES_COMMITMENT is the commitment of the output notes.
    /// - FINAL_ACCOUNT_COMMITMENT is the final account commitment of the account that the
    ///   transaction is being executed against.
    /// - tx_expiration_block_num is the block height at which the transaction will become expired,
    ///   defined by the sum of the execution block ref and the transaction's block expiration delta
    ///   (if set during transaction execution).
    ///
    /// The actual data describing the new account state and output notes is expected to be located
    /// in the provided advice map under keys `OUTPUT_NOTES_COMMITMENT` and
    /// `FINAL_ACCOUNT_COMMITMENT`.
    pub fn from_transaction_parts(
        stack: &StackOutputs,
        adv_map: &AdviceMap,
        output_notes: Vec<OutputNote>,
    ) -> Result<TransactionOutputs, TransactionOutputError> {
        let (final_account_commitment, output_notes_commitment, expiration_block_num) =
            Self::parse_output_stack(stack)?;

        // parse final account state
        let final_account_data = adv_map
            .get(&final_account_commitment)
            .ok_or(TransactionOutputError::FinalAccountHashMissingInAdviceMap)?;
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
            output_notes,
            expiration_block_num,
        })
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
        let source_manager = Arc::new(DefaultSourceManager::default());
        let kernel_library = Self::kernel_as_library();

        Assembler::with_kernel(source_manager, Self::kernel())
            .with_library(StdLibrary::default())
            .expect("failed to load std-lib")
            .with_library(MidenLib::default())
            .expect("failed to load miden-lib")
            .with_library(kernel_library)
            .expect("failed to load kernel library (/lib)")
    }

    /// Returns the testing assembler, and additionally contains the library for
    /// [AccountCode::mock_library()], which is a mock wallet used in tests.
    pub fn testing_assembler_with_mock_account() -> Assembler {
        let assembler = Self::testing_assembler().with_debug_mode(true);
        let library = AccountCode::mock_library(assembler.clone());

        assembler.with_library(library).expect("failed to add mock account code")
    }
}
