use alloc::{collections::BTreeSet, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    Felt, MAX_TX_EXECUTION_CYCLES, MIN_TX_EXECUTION_CYCLES,
    account::AccountId,
    assembly::SourceManager,
    block::{BlockHeader, BlockNumber},
    note::{NoteId, NoteScript},
    transaction::{
        AccountInputs, ExecutedTransaction, InputNote, InputNotes, TransactionArgs,
        TransactionInputs, TransactionScript,
    },
    vm::StackOutputs,
};
use vm_processor::{AdviceInputs, Process};
pub use vm_processor::{ExecutionOptions, MastForestStore};
use winter_maybe_async::{maybe_async, maybe_await};

use super::{TransactionExecutorError, TransactionHost};
use crate::{auth::TransactionAuthenticator, host::ScriptMastForestStore};

mod data_store;
pub use data_store::DataStore;

mod notes_checker;
pub use notes_checker::{NoteConsumptionChecker, NoteInputsCheck};

// TRANSACTION EXECUTOR
// ================================================================================================

/// The transaction executor is responsible for executing Miden blockchain transactions.
///
/// Transaction execution consists of the following steps:
/// - Fetch the data required to execute a transaction from the [DataStore].
/// - Execute the transaction program and create an [ExecutedTransaction].
///
/// The transaction executor uses dynamic dispatch with trait objects for the [DataStore] and
/// [TransactionAuthenticator], allowing it to be used with different backend implementations.
/// At the moment of execution, the [DataStore] is expected to provide all required MAST nodes.
pub struct TransactionExecutor<'store, 'auth> {
    data_store: &'store dyn DataStore,
    authenticator: Option<&'auth dyn TransactionAuthenticator>,
    exec_options: ExecutionOptions,
}

impl<'store, 'auth> TransactionExecutor<'store, 'auth> {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Creates a new [TransactionExecutor] instance with the specified [DataStore] and
    /// [TransactionAuthenticator].
    pub fn new(
        data_store: &'store dyn DataStore,
        authenticator: Option<&'auth dyn TransactionAuthenticator>,
    ) -> Self {
        const _: () = assert!(MIN_TX_EXECUTION_CYCLES <= MAX_TX_EXECUTION_CYCLES);

        Self {
            data_store,
            authenticator,
            exec_options: ExecutionOptions::new(
                Some(MAX_TX_EXECUTION_CYCLES),
                MIN_TX_EXECUTION_CYCLES,
                false,
                false,
            )
            .expect("Must not fail while max cycles is more than min trace length"),
        }
    }

    /// Creates a new [TransactionExecutor] instance with the specified [DataStore],
    /// [TransactionAuthenticator] and [ExecutionOptions].
    ///
    /// The specified cycle values (`max_cycles` and `expected_cycles`) in the [ExecutionOptions]
    /// must be within the range [`MIN_TX_EXECUTION_CYCLES`] and [`MAX_TX_EXECUTION_CYCLES`].
    pub fn with_options(
        data_store: &'store dyn DataStore,
        authenticator: Option<&'auth dyn TransactionAuthenticator>,
        exec_options: ExecutionOptions,
    ) -> Result<Self, TransactionExecutorError> {
        validate_num_cycles(exec_options.max_cycles())?;
        validate_num_cycles(exec_options.expected_cycles())?;

        Ok(Self { data_store, authenticator, exec_options })
    }

    /// Puts the [TransactionExecutor] into debug mode.
    ///
    /// When transaction executor is in debug mode, all transaction-related code (note scripts,
    /// account code) will be compiled and executed in debug mode. This will ensure that all debug
    /// instructions present in the original source code are executed.
    pub fn with_debug_mode(mut self) -> Self {
        self.exec_options = self.exec_options.with_debugging(true);
        self
    }

    /// Enables tracing for the created instance of [TransactionExecutor].
    ///
    /// When tracing is enabled, the executor will receive tracing events as various stages of the
    /// transaction kernel complete. This enables collecting basic stats about how long different
    /// stages of transaction execution take.
    pub fn with_tracing(mut self) -> Self {
        self.exec_options = self.exec_options.with_tracing();
        self
    }

    // TRANSACTION EXECUTION
    // --------------------------------------------------------------------------------------------

    /// Prepares and executes a transaction specified by the provided arguments and returns an
    /// [`ExecutedTransaction`].
    ///
    /// The method first fetches the data required to execute the transaction from the [`DataStore`]
    /// and compile the transaction into an executable program. In particular, it fetches the
    /// account identified by the account ID from the store as well as `block_ref`, the header of
    /// the reference block of the transaction and the set of headers from the blocks in which the
    /// provided `notes` were created. Then, it executes the transaction program and creates an
    /// [`ExecutedTransaction`].
    ///
    /// The `source_manager` is used to map potential errors back to their source code. To get the
    /// most value out of it, use the source manager from the
    /// [`Assembler`](miden_objects::assembly::Assembler) that assembled the Miden Assembly code
    /// that should be debugged, e.g. account components, note scripts or transaction scripts. If
    /// no error-to-source mapping is desired, a default source manager can be passed, e.g.
    /// [`DefaultSourceManager::default`](miden_objects::assembly::DefaultSourceManager::default).
    ///
    /// # Errors:
    ///
    /// Returns an error if:
    /// - If required data can not be fetched from the [`DataStore`].
    /// - If the transaction arguments contain foreign account data not anchored in the reference
    ///   block.
    /// - If any input notes were created in block numbers higher than the reference block.
    #[maybe_async]
    pub fn execute_transaction(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let mut ref_blocks = validate_input_notes(&notes, block_ref)?;
        ref_blocks.insert(block_ref);

        let (account, seed, ref_block, mmr) =
            maybe_await!(self.data_store.get_transaction_inputs(account_id, ref_blocks))
                .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;

        validate_account_inputs(&tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, notes)
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, None);

        let mut advice_inputs = advice_inputs.into_advice_inputs();

        let script_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            tx_inputs.input_notes().iter().map(|n| n.note().script()),
        );

        let mut host = TransactionHost::new(
            &tx_inputs.account().into(),
            &mut advice_inputs,
            self.data_store,
            script_mast_store,
            self.authenticator,
            tx_args.foreign_account_code_commitments(),
        )
        .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        // Execute the transaction kernel
        let trace = vm_processor::execute(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs,
            &mut host,
            self.exec_options,
            source_manager,
        )
        .map_err(TransactionExecutorError::TransactionProgramExecutionFailed)?;
        let (stack_outputs, advice_provider) = trace.into_outputs();

        // The stack is not necessary since it is being reconstructed when re-executing.
        let advice_inputs = AdviceInputs::default()
            .with_map(advice_provider.map)
            .with_merkle_store(advice_provider.store);

        build_executed_transaction(advice_inputs, tx_args, tx_inputs, stack_outputs, host)
    }

    // SCRIPT EXECUTION
    // --------------------------------------------------------------------------------------------

    /// Executes an arbitrary script against the given account and returns the stack state at the
    /// end of execution.
    ///
    /// The `source_manager` is used to map potential errors back to their source code. To get the
    /// most value out of it, use the source manager from the
    /// [`Assembler`](miden_objects::assembly::Assembler) that assembled the Miden Assembly code
    /// that should be debugged, e.g. account components, note scripts or transaction scripts. If
    /// no error-to-source mapping is desired, a default source manager can be passed, e.g.
    /// [`DefaultSourceManager::default`](miden_objects::assembly::DefaultSourceManager::default).
    ///
    /// # Errors:
    /// Returns an error if:
    /// - If required data can not be fetched from the [DataStore].
    /// - If the transaction host can not be created from the provided values.
    /// - If the execution of the provided program fails.
    #[maybe_async]
    pub fn execute_tx_view_script(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        tx_script: TransactionScript,
        advice_inputs: AdviceInputs,
        foreign_account_inputs: Vec<AccountInputs>,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<[Felt; 16], TransactionExecutorError> {
        let ref_blocks = [block_ref].into_iter().collect();
        let (account, seed, ref_block, mmr) =
            maybe_await!(self.data_store.get_transaction_inputs(account_id, ref_blocks))
                .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;
        let tx_args = TransactionArgs::new(Default::default(), foreign_account_inputs)
            .with_tx_script(tx_script);

        validate_account_inputs(&tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, Default::default())
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_inputs));
        let mut advice_inputs = advice_inputs.into_advice_inputs();

        let scripts_mast_store =
            ScriptMastForestStore::new(tx_args.tx_script(), core::iter::empty::<&NoteScript>());

        let mut host = TransactionHost::new(
            &tx_inputs.account().into(),
            &mut advice_inputs,
            self.data_store,
            scripts_mast_store,
            self.authenticator,
            tx_args.foreign_account_code_commitments(),
        )
        .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        let mut process = Process::new(
            TransactionKernel::tx_script_main().kernel().clone(),
            stack_inputs,
            advice_inputs,
            self.exec_options,
        )
        .with_source_manager(source_manager);
        let stack_outputs = process
            .execute(&TransactionKernel::tx_script_main(), &mut host)
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed)?;

        Ok(*stack_outputs)
    }

    // CHECK CONSUMABILITY
    // ============================================================================================

    /// Executes the transaction with specified notes, returning the [NoteAccountExecution::Success]
    /// if all notes has been consumed successfully and [NoteAccountExecution::Failure] if some note
    /// returned an error.
    ///
    /// The `source_manager` is used to map potential errors back to their source code. To get the
    /// most value out of it, use the source manager from the
    /// [`Assembler`](miden_objects::assembly::Assembler) that assembled the Miden Assembly code
    /// that should be debugged, e.g. account components, note scripts or transaction scripts. If
    /// no error-to-source mapping is desired, a default source manager can be passed, e.g.
    /// [`DefaultSourceManager::default`](miden_objects::assembly::DefaultSourceManager::default).
    ///
    /// # Errors:
    /// Returns an error if:
    /// - If required data can not be fetched from the [DataStore].
    /// - If the transaction host can not be created from the provided values.
    /// - If the execution of the provided program fails on the stage other than note execution.
    #[maybe_async]
    pub(crate) fn try_execute_notes(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<NoteAccountExecution, TransactionExecutorError> {
        let mut ref_blocks = validate_input_notes(&notes, block_ref)?;
        ref_blocks.insert(block_ref);

        let (account, seed, ref_block, mmr) =
            maybe_await!(self.data_store.get_transaction_inputs(account_id, ref_blocks))
                .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;

        validate_account_inputs(&tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, notes)
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, None);

        let mut advice_inputs = advice_inputs.into_advice_inputs();

        let scripts_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            tx_inputs.input_notes().iter().map(|n| n.note().script()),
        );

        let mut host = TransactionHost::new(
            &tx_inputs.account().into(),
            &mut advice_inputs,
            self.data_store,
            scripts_mast_store,
            self.authenticator,
            tx_args.foreign_account_code_commitments(),
        )
        .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        // execute the transaction kernel
        let result = vm_processor::execute(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs,
            &mut host,
            self.exec_options,
            source_manager,
        )
        .map_err(TransactionExecutorError::TransactionProgramExecutionFailed);

        match result {
            Ok(_) => Ok(NoteAccountExecution::Success),
            Err(tx_execution_error) => {
                let notes = host.tx_progress().note_execution();

                // empty notes vector means that we didn't process the notes, so an error
                // occurred somewhere else
                if notes.is_empty() {
                    return Err(tx_execution_error);
                }

                let ((last_note, last_note_interval), success_notes) = notes
                    .split_last()
                    .expect("notes vector should not be empty because we just checked");

                // if the interval end of the last note is specified, then an error occurred after
                // notes processing
                if last_note_interval.end().is_some() {
                    return Err(tx_execution_error);
                }

                Ok(NoteAccountExecution::Failure {
                    failed_note_id: *last_note,
                    successful_notes: success_notes.iter().map(|(note, _)| *note).collect(),
                    error: Some(tx_execution_error),
                })
            },
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Creates a new [ExecutedTransaction] from the provided data.
fn build_executed_transaction(
    mut advice_inputs: AdviceInputs,
    tx_args: TransactionArgs,
    tx_inputs: TransactionInputs,
    stack_outputs: StackOutputs,
    host: TransactionHost,
) -> Result<ExecutedTransaction, TransactionExecutorError> {
    let (account_delta, output_notes, generated_signatures, tx_progress) = host.into_parts();

    let tx_outputs =
        TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs.map, output_notes)
            .map_err(TransactionExecutorError::TransactionOutputConstructionFailed)?;

    let initial_account = tx_inputs.account();
    let final_account = &tx_outputs.account;

    let host_delta_commitment = account_delta.commitment();
    if tx_outputs.account_delta_commitment != host_delta_commitment {
        return Err(TransactionExecutorError::InconsistentAccountDeltaCommitment {
            in_kernel_commitment: tx_outputs.account_delta_commitment,
            host_commitment: host_delta_commitment,
        });
    }

    if initial_account.id() != final_account.id() {
        return Err(TransactionExecutorError::InconsistentAccountId {
            input_id: initial_account.id(),
            output_id: final_account.id(),
        });
    }

    // make sure nonce delta was computed correctly
    let nonce_delta = final_account.nonce() - initial_account.nonce();
    if nonce_delta != account_delta.nonce_delta() {
        return Err(TransactionExecutorError::InconsistentAccountNonceDelta {
            expected: nonce_delta,
            actual: account_delta.nonce_delta(),
        });
    }

    // introduce generated signatures into the witness inputs
    advice_inputs.extend_map(generated_signatures);

    Ok(ExecutedTransaction::new(
        tx_inputs,
        tx_outputs,
        account_delta,
        tx_args,
        advice_inputs,
        tx_progress.into(),
    ))
}

/// Validates the account inputs against the reference block header.
fn validate_account_inputs(
    tx_args: &TransactionArgs,
    ref_block: &BlockHeader,
) -> Result<(), TransactionExecutorError> {
    // Validate that foreign account inputs are anchored in the reference block
    for foreign_account in tx_args.foreign_account_inputs() {
        let computed_account_root = foreign_account.compute_account_root().map_err(|err| {
            TransactionExecutorError::InvalidAccountWitness(foreign_account.id(), err)
        })?;
        if computed_account_root != ref_block.account_root() {
            return Err(TransactionExecutorError::ForeignAccountNotAnchoredInReference(
                foreign_account.id(),
            ));
        }
    }
    Ok(())
}

/// Validates that input notes were not created after the reference block.
///
/// Returns the set of block numbers required to execute the provided notes.
fn validate_input_notes(
    notes: &InputNotes<InputNote>,
    block_ref: BlockNumber,
) -> Result<BTreeSet<BlockNumber>, TransactionExecutorError> {
    // Validate that notes were not created after the reference, and build the set of required
    // block numbers
    let mut ref_blocks: BTreeSet<BlockNumber> = BTreeSet::new();
    for note in notes.iter() {
        if let Some(location) = note.location() {
            if location.block_num() > block_ref {
                return Err(TransactionExecutorError::NoteBlockPastReferenceBlock(
                    note.id(),
                    block_ref,
                ));
            }
            ref_blocks.insert(location.block_num());
        }
    }

    Ok(ref_blocks)
}

/// Validates that the number of cycles specified is within the allowed range.
fn validate_num_cycles(num_cycles: u32) -> Result<(), TransactionExecutorError> {
    if !(MIN_TX_EXECUTION_CYCLES..=MAX_TX_EXECUTION_CYCLES).contains(&num_cycles) {
        Err(TransactionExecutorError::InvalidExecutionOptionsCycles {
            min_cycles: MIN_TX_EXECUTION_CYCLES,
            max_cycles: MAX_TX_EXECUTION_CYCLES,
            actual: num_cycles,
        })
    } else {
        Ok(())
    }
}

// HELPER ENUM
// ================================================================================================

/// Describes whether a transaction with a specified set of notes could be executed against target
/// account.
///
/// [NoteAccountExecution::Failure] holds data for error handling: `failing_note_id` is an ID of a
/// failing note and `successful_notes` is a vector of note IDs which were successfully executed.
#[derive(Debug)]
pub enum NoteAccountExecution {
    Success,
    Failure {
        failed_note_id: NoteId,
        successful_notes: Vec<NoteId>,
        error: Option<TransactionExecutorError>,
    },
}
