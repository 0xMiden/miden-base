use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::errors::TransactionKernelError;
use miden_lib::transaction::TransactionKernel;
use miden_objects::account::AccountId;
use miden_objects::assembly::SourceManager;
use miden_objects::block::{BlockHeader, BlockNumber};
use miden_objects::note::{Note, NoteScript};
use miden_objects::transaction::{
    AccountInputs,
    ExecutedTransaction,
    InputNote,
    InputNotes,
    TransactionArgs,
    TransactionInputs,
    TransactionScript,
};
use miden_objects::vm::StackOutputs;
use miden_objects::{Felt, MAX_TX_EXECUTION_CYCLES, MIN_TX_EXECUTION_CYCLES};
use vm_processor::fast::FastProcessor;
use vm_processor::{AdviceInputs, ExecutionError, StackInputs};
pub use vm_processor::{ExecutionOptions, MastForestStore};

use super::TransactionExecutorError;
use crate::auth::TransactionAuthenticator;
use crate::host::{AccountProcedureIndexMap, ScriptMastForestStore};

mod exec_host;
pub use exec_host::TransactionExecutorHost;

mod data_store;
pub use data_store::DataStore;

mod notes_checker;
pub use notes_checker::NoteConsumptionChecker;

// NOTE CONSUMPTION INFO
// ================================================================================================

/// Represents a failed note consumption.
#[derive(Debug)]
#[non_exhaustive]
pub struct FailedNote {
    pub note: Note,
    pub error: TransactionExecutorError,
}

/// Contains information about the successful and failed consumption of notes.
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct NoteConsumptionInfo {
    pub successful: Vec<Note>,
    pub failed: Vec<FailedNote>,
    pub unattempted: Vec<Note>,
}

impl NoteConsumptionInfo {
    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful notes.
    pub fn new_successful(successful: Vec<Note>) -> Self {
        Self { successful, ..Default::default() }
    }

    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful and failed notes.
    pub fn new(successful: Vec<Note>, failed: Vec<FailedNote>, unattempted: Vec<Note>) -> Self {
        Self { successful, failed, unattempted }
    }
}

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
pub struct TransactionExecutor<'store, 'auth, STORE: 'store, AUTH: 'auth> {
    data_store: &'store STORE,
    authenticator: Option<&'auth AUTH>,
    exec_options: ExecutionOptions,
}

impl<'store, 'auth, STORE, AUTH> TransactionExecutor<'store, 'auth, STORE, AUTH>
where
    STORE: DataStore + 'store + Sync,
    AUTH: TransactionAuthenticator + 'auth + Sync,
{
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Creates a new [TransactionExecutor] instance with the specified [DataStore] and
    /// [TransactionAuthenticator].
    pub fn new(data_store: &'store STORE, authenticator: Option<&'auth AUTH>) -> Self {
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
        data_store: &'store STORE,
        authenticator: Option<&'auth AUTH>,
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
    pub async fn execute_transaction(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        // TODO: SourceManager: Pass source manager to host once refactored.
        _source_manager: Arc<dyn SourceManager + Send + Sync>,
    ) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let mut ref_blocks = validate_input_notes(&notes, block_ref)?;
        ref_blocks.insert(block_ref);

        let (account, seed, ref_block, mmr) = self
            .data_store
            .get_transaction_inputs(account_id, ref_blocks)
            .await
            .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;

        validate_account_inputs(&tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, notes)
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, None)
                .map_err(TransactionExecutorError::ConflictingAdviceMapEntry)?;
        // This reverses the stack inputs (even though it doesn't look like it does) because the
        // fast processor expects the reverse order.
        //
        // Once we use the FastProcessor for execution and proving, we can change the way these
        // inputs are constructed in TransactionKernel::prepare_inputs.
        let stack_inputs = StackInputs::new(stack_inputs.iter().copied().collect()).unwrap();

        let input_notes = tx_inputs.input_notes();

        let script_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            input_notes.iter().map(|n| n.note().script()),
        );

        let acct_procedure_index_map =
            AccountProcedureIndexMap::from_transaction_params(&tx_inputs, &tx_args, &advice_inputs)
                .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        let mut host = TransactionExecutorHost::new(
            &tx_inputs.account().into(),
            input_notes.clone(),
            self.data_store,
            script_mast_store,
            acct_procedure_index_map,
            self.authenticator,
        );

        let advice_inputs = advice_inputs.into_advice_inputs();

        let processor = FastProcessor::new_debug(stack_inputs.as_slice(), advice_inputs);
        let (stack_outputs, advice_provider) = processor
            .execute(&TransactionKernel::main(), &mut host)
            .await
            .map_err(map_execution_error)?;

        // The stack is not necessary since it is being reconstructed when re-executing.
        let (_stack, advice_map, merkle_store) = advice_provider.into_parts();
        let advice_inputs = AdviceInputs {
            map: advice_map,
            store: merkle_store,
            ..Default::default()
        };

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
    pub async fn execute_tx_view_script(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        tx_script: TransactionScript,
        advice_inputs: AdviceInputs,
        foreign_account_inputs: Vec<AccountInputs>,
        // TODO: SourceManager: Pass source manager to host once refactored.
        _source_manager: Arc<dyn SourceManager + Send + Sync>,
    ) -> Result<[Felt; 16], TransactionExecutorError> {
        let ref_blocks = [block_ref].into_iter().collect();
        let (account, seed, ref_block, mmr) = self
            .data_store
            .get_transaction_inputs(account_id, ref_blocks)
            .await
            .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;
        let tx_args = TransactionArgs::new(Default::default(), foreign_account_inputs)
            .with_tx_script(tx_script);

        validate_account_inputs(&tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, Default::default())
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_inputs))
                .map_err(TransactionExecutorError::ConflictingAdviceMapEntry)?;
        // This reverses the stack inputs (even though it doesn't look like it does) because the
        // fast processor expects the reverse order.
        let stack_inputs = StackInputs::new(stack_inputs.iter().copied().collect()).unwrap();

        let scripts_mast_store =
            ScriptMastForestStore::new(tx_args.tx_script(), core::iter::empty::<&NoteScript>());

        let acct_procedure_index_map =
            AccountProcedureIndexMap::from_transaction_params(&tx_inputs, &tx_args, &advice_inputs)
                .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        let mut host = TransactionExecutorHost::new(
            &tx_inputs.account().into(),
            tx_inputs.input_notes().clone(),
            self.data_store,
            scripts_mast_store,
            acct_procedure_index_map,
            self.authenticator,
        );

        let advice_inputs = advice_inputs.into_advice_inputs();

        let processor =
            FastProcessor::new_with_advice_inputs(stack_inputs.as_slice(), advice_inputs);
        let (stack_outputs, _advice_provider) = processor
            .execute(&TransactionKernel::tx_script_main(), &mut host)
            .await
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed)?;

        Ok(*stack_outputs)
    }

    // CHECK CONSUMABILITY
    // ============================================================================================

    /// Validates input notes, transaction inputs, and account inputs before executing the
    /// transaction with specified notes. Keeps track and returns both successfully consumed notes
    /// as well as notes that failed to be consumed.
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
    /// - If required data can not be fetched from the [`DataStore`].
    /// - If the transaction host can not be created from the provided values.
    /// - If the execution of the provided program fails on the stage other than note execution.
    pub(crate) async fn try_execute_notes(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        if notes.is_empty() {
            return Ok(NoteConsumptionInfo::default());
        }
        // Validate input notes.
        let mut ref_blocks = validate_input_notes(&notes, block_ref)?;
        ref_blocks.insert(block_ref);

        // Validate account inputs.
        let (account, seed, ref_block, mmr) = self
            .data_store
            .get_transaction_inputs(account_id, ref_blocks)
            .await
            .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;
        validate_account_inputs(&tx_args, &ref_block)?;

        // Prepare transaction inputs.
        let tx_inputs = TransactionInputs::new(account, seed, ref_block, mmr, notes)
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;
        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, None)
                .map_err(TransactionExecutorError::ConflictingAdviceMapEntry)?;
        // This reverses the stack inputs (even though it doesn't look like it does) because the
        // fast processor expects the reverse order.
        let stack_inputs = StackInputs::new(stack_inputs.iter().copied().collect()).unwrap();

        // Prepare host for transaction execution.
        let input_notes = tx_inputs.input_notes();
        let scripts_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            input_notes.iter().map(|n| n.note().script()),
        );
        let acct_procedure_index_map =
            AccountProcedureIndexMap::from_transaction_params(&tx_inputs, &tx_args, &advice_inputs)
                .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;
        let mut host = TransactionExecutorHost::new(
            &tx_inputs.account().into(),
            input_notes.clone(),
            self.data_store,
            scripts_mast_store,
            acct_procedure_index_map,
            self.authenticator,
        );
        let advice_inputs = advice_inputs.into_advice_inputs();

        let processor =
            FastProcessor::new_with_advice_inputs(stack_inputs.as_slice(), advice_inputs);
        let result = processor
            .execute(&TransactionKernel::main(), &mut host)
            .await
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed);

        let (_, _, _, _, input_notes) = tx_inputs.into_parts();
        match result {
            Ok(_) => {
                // Return all the input notes as successful.
                Ok(NoteConsumptionInfo::new_successful(
                    input_notes.into_iter().map(|note| note.into_note()).collect::<Vec<_>>(),
                ))
            },
            Err(error) => {
                let notes = host.tx_progress().note_execution();

                // Empty notes vector means that we didn't process the notes, so an error
                // occurred.
                if notes.is_empty() {
                    return Err(error);
                }

                let ((_failed_note, last_note_interval), success_notes) =
                    notes.split_last().expect("notes vector is not empty because of earlier check");

                // If the interval end of the last note is specified, then an error occurred after
                // notes processing.
                if last_note_interval.end().is_some() {
                    return Err(error);
                }

                // Partition the input notes into successful and failed results.
                let (successful, failed, unattempted) =
                    split_at(input_notes.into_vec(), success_notes.len());
                let successful =
                    successful.into_iter().map(InputNote::into_note).collect::<Vec<_>>();
                let failed = vec![FailedNote { error, note: failed.into_note() }];
                let unattempted =
                    unattempted.into_iter().map(InputNote::into_note).collect::<Vec<_>>();

                // Return information about all the consumed notes.
                Ok(NoteConsumptionInfo::new(successful, failed, unattempted))
            },
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Splits a vector into three parts based on a given separator index.
///
/// # Safety
///
/// The caller must ensure that the provided `separator_index` is within the bounds of the input
/// vector.
fn split_at<T>(mut vec: Vec<T>, index: usize) -> (Vec<T>, T, Vec<T>) {
    // Drain elements after separator into after vector
    let after: Vec<T> = vec.drain(index + 1..).collect();

    // Remove and get the separator
    let separator = vec.pop().expect("provided separator index in bounds");

    // vec now contains only the before part
    let before = vec;

    (before, separator, after)
}

/// Creates a new [ExecutedTransaction] from the provided data.
fn build_executed_transaction<STORE: DataStore + Sync, AUTH: TransactionAuthenticator + Sync>(
    mut advice_inputs: AdviceInputs,
    tx_args: TransactionArgs,
    tx_inputs: TransactionInputs,
    stack_outputs: StackOutputs,
    host: TransactionExecutorHost<STORE, AUTH>,
) -> Result<ExecutedTransaction, TransactionExecutorError> {
    let (account_delta, output_notes, generated_signatures, tx_progress) = host.into_parts();

    let tx_outputs =
        TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
            .map_err(TransactionExecutorError::TransactionOutputConstructionFailed)?;

    let initial_account = tx_inputs.account();
    let final_account = &tx_outputs.account;

    let host_delta_commitment = account_delta.to_commitment();
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
    advice_inputs.map.extend(generated_signatures);

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

/// Remaps an execution error to a transaction executor error.
///
/// - If the inner error is [`TransactionKernelError::Unauthorized`], it is remapped to
///   [`TransactionExecutorError::Unauthorized`].
/// - Otherwise, the execution error is wrapped in
///   [`TransactionExecutorError::TransactionProgramExecutionFailed`].
fn map_execution_error(exec_err: ExecutionError) -> TransactionExecutorError {
    match exec_err {
        ExecutionError::EventError { ref error, .. } => {
            match error.downcast_ref::<TransactionKernelError>() {
                Some(TransactionKernelError::Unauthorized(summary)) => {
                    TransactionExecutorError::Unauthorized(summary.clone())
                },
                _ => TransactionExecutorError::TransactionProgramExecutionFailed(exec_err),
            }
        },
        _ => TransactionExecutorError::TransactionProgramExecutionFailed(exec_err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[case::odd(vec![1, 2, 3, 4, 5], 2, vec![1, 2], 3, vec![4, 5])]
    #[case::even(vec![1, 2, 3, 4, 5, 6], 2, vec![1, 2], 3, vec![4, 5, 6])]
    #[case::start(vec![1, 2, 3, 4, 5], 0, vec![], 1, vec![2, 3, 4, 5])]
    #[case::end(vec![1, 2, 3, 4, 5], 4, vec![1, 2, 3, 4], 5, vec![])]
    #[test]
    fn splitting(
        #[case] input: Vec<u32>,
        #[case] separator_index: usize,
        #[case] start: Vec<u32>,
        #[case] separator: u32,
        #[case] end: Vec<u32>,
    ) {
        let (left, mid, right) = split_at(input, separator_index);
        assert_eq!(left, start);
        assert_eq!(mid, separator);
        assert_eq!(right, end);
    }
}
