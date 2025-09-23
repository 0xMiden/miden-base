use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::AccountId;
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::assembly::debuginfo::SourceManagerSync;
use miden_objects::asset::Asset;
use miden_objects::block::{BlockHeader, BlockNumber};
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
use miden_processor::fast::FastProcessor;
use miden_processor::{AdviceInputs, ExecutionError, StackInputs};
pub use miden_processor::{ExecutionOptions, MastForestStore};

use super::TransactionExecutorError;
use crate::auth::TransactionAuthenticator;
use crate::errors::TransactionKernelError;
use crate::host::{AccountProcedureIndexMap, ScriptMastForestStore};

mod exec_host;
pub use exec_host::TransactionExecutorHost;

mod data_store;
pub use data_store::DataStore;

mod notes_checker;
pub use notes_checker::{
    FailedNote,
    MAX_NUM_CHECKER_NOTES,
    NoteConsumptionChecker,
    NoteConsumptionInfo,
};

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
    source_manager: Arc<dyn SourceManagerSync>,
    exec_options: ExecutionOptions,
}

impl<'store, 'auth, STORE, AUTH> TransactionExecutor<'store, 'auth, STORE, AUTH>
where
    STORE: DataStore + 'store + Sync,
    AUTH: TransactionAuthenticator + 'auth + Sync,
{
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [TransactionExecutor] instance with the specified [DataStore].
    ///
    /// The created executor will not have the authenticator or source manager set, and tracing and
    /// debug mode will be turned off.
    pub fn new(data_store: &'store STORE) -> Self {
        const _: () = assert!(MIN_TX_EXECUTION_CYCLES <= MAX_TX_EXECUTION_CYCLES);
        TransactionExecutor {
            data_store,
            authenticator: None,
            source_manager: Arc::new(DefaultSourceManager::default()),
            exec_options: ExecutionOptions::new(
                Some(MAX_TX_EXECUTION_CYCLES),
                MIN_TX_EXECUTION_CYCLES,
                false,
                false,
            )
            .expect("Must not fail while max cycles is more than min trace length"),
        }
    }

    /// Adds the specified [TransactionAuthenticator] to the executor and returns the resulting
    /// executor.
    ///
    /// This will overwrite any previously set authenticator.
    #[must_use]
    pub fn with_authenticator(mut self, authenticator: &'auth AUTH) -> Self {
        self.authenticator = Some(authenticator);
        self
    }

    /// Adds the specified source manager to the executor and returns the resulting executor.
    ///
    /// The `source_manager` is used to map potential errors back to their source code. To get the
    /// most value out of it, use the same source manager as was used with the
    /// [`Assembler`](miden_objects::assembly::Assembler) that assembled the Miden Assembly code
    /// that should be debugged, e.g. account components, note scripts or transaction scripts.
    ///
    /// This will overwrite any previously set source manager.
    #[must_use]
    pub fn with_source_manager(mut self, source_manager: Arc<dyn SourceManagerSync>) -> Self {
        self.source_manager = source_manager;
        self
    }

    /// Sets the [ExecutionOptions] for the executor to the provided options and returns the
    /// resulting executor.
    ///
    /// # Errors
    /// Returns an error if the specified cycle values (`max_cycles` and `expected_cycles`) in
    /// the [ExecutionOptions] are not within the range [`MIN_TX_EXECUTION_CYCLES`] and
    /// [`MAX_TX_EXECUTION_CYCLES`].
    pub fn with_options(
        mut self,
        exec_options: ExecutionOptions,
    ) -> Result<Self, TransactionExecutorError> {
        validate_num_cycles(exec_options.max_cycles())?;
        validate_num_cycles(exec_options.expected_cycles())?;

        self.exec_options = exec_options;
        Ok(self)
    }

    /// Puts the [TransactionExecutor] into debug mode and returns the resulting executor.
    ///
    /// When transaction executor is in debug mode, all transaction-related code (note scripts,
    /// account code) will be compiled and executed in debug mode. This will ensure that all debug
    /// instructions present in the original source code are executed.
    #[must_use]
    pub fn with_debug_mode(mut self) -> Self {
        self.exec_options = self.exec_options.with_debugging(true);
        self
    }

    /// Enables tracing for the created instance of [TransactionExecutor] and returns the resulting
    /// executor.
    ///
    /// When tracing is enabled, the executor will receive tracing events as various stages of the
    /// transaction kernel complete. This enables collecting basic stats about how long different
    /// stages of transaction execution take.
    #[must_use]
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
    ) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let tx_inputs =
            self.validate_transaction_notes(account_id, block_ref, notes, &tx_args).await?;

        let (mut host, stack_inputs, advice_inputs) =
            self.prepare_transaction(&tx_inputs, &tx_args, None).await?;

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
    ) -> Result<[Felt; 16], TransactionExecutorError> {
        let tx_args = TransactionArgs::new(Default::default(), foreign_account_inputs)
            .with_tx_script(tx_script);

        let notes = InputNotes::default();
        let tx_inputs =
            self.validate_transaction_notes(account_id, block_ref, notes, &tx_args).await?;

        let (mut host, stack_inputs, advice_inputs) =
            self.prepare_transaction(&tx_inputs, &tx_args, Some(advice_inputs)).await?;

        let processor =
            FastProcessor::new_with_advice_inputs(stack_inputs.as_slice(), advice_inputs);
        let (stack_outputs, _advice_provider) = processor
            .execute(&TransactionKernel::tx_script_main(), &mut host)
            .await
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed)?;

        Ok(*stack_outputs)
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    // Validates input notes and account inputs after retrieving transaction inputs from the store.
    //
    // This method has a one-to-many call relationship with the `prepare_transaction` method. This
    // method needs to be called only once in order to allow many transactions to be prepared based
    // on its outputs.
    async fn validate_transaction_notes(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: InputNotes<InputNote>,
        tx_args: &TransactionArgs,
    ) -> Result<TransactionInputs, TransactionExecutorError> {
        let mut ref_blocks = validate_input_notes(&input_notes, block_ref)?;
        ref_blocks.insert(block_ref);

        let (account, ref_block, mmr) = self
            .data_store
            .get_transaction_inputs(account_id, ref_blocks)
            .await
            .map_err(TransactionExecutorError::FetchTransactionInputsFailed)?;

        validate_account_inputs(tx_args, &ref_block)?;

        let tx_inputs = TransactionInputs::new(account, ref_block, mmr, input_notes)
            .map_err(TransactionExecutorError::InvalidTransactionInputs)?;

        Ok(tx_inputs)
    }

    /// Prepares the data needed for transaction execution.
    ///
    /// Preparation includes loading transaction inputs from the data store, validating them, and
    /// instantiating a transaction host.
    async fn prepare_transaction(
        &self,
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
        init_advice_inputs: Option<AdviceInputs>,
    ) -> Result<
        (TransactionExecutorHost<'store, 'auth, STORE, AUTH>, StackInputs, AdviceInputs),
        TransactionExecutorError,
    > {
        let (stack_inputs, tx_advice_inputs) =
            TransactionKernel::prepare_inputs(tx_inputs, tx_args, init_advice_inputs)
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

        // To start executing the transaction, the procedure index map only needs to contain the
        // native account's procedures. Foreign accounts are inserted into the map on first access.
        let account_procedure_index_map =
            AccountProcedureIndexMap::new([tx_inputs.account().code()])
                .map_err(TransactionExecutorError::TransactionHostCreationFailed)?;

        let host = TransactionExecutorHost::new(
            tx_inputs.account(),
            input_notes.clone(),
            self.data_store,
            script_mast_store,
            account_procedure_index_map,
            self.authenticator,
            tx_inputs.block_header().block_num(),
            self.source_manager.clone(),
        );

        let advice_inputs = tx_advice_inputs.into_advice_inputs();

        Ok((host, stack_inputs, advice_inputs))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Creates a new [ExecutedTransaction] from the provided data.
fn build_executed_transaction<STORE: DataStore + Sync, AUTH: TransactionAuthenticator + Sync>(
    mut advice_inputs: AdviceInputs,
    tx_args: TransactionArgs,
    tx_inputs: TransactionInputs,
    stack_outputs: StackOutputs,
    host: TransactionExecutorHost<STORE, AUTH>,
) -> Result<ExecutedTransaction, TransactionExecutorError> {
    // Note that the account delta does not contain the removed transaction fee, so it is the
    // "pre-fee" delta of the transaction.
    let (
        pre_fee_account_delta,
        _input_notes,
        output_notes,
        accessed_foreign_account_code,
        generated_signatures,
        tx_progress,
    ) = host.into_parts();

    let tx_outputs =
        TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
            .map_err(TransactionExecutorError::TransactionOutputConstructionFailed)?;

    let pre_fee_delta_commitment = pre_fee_account_delta.to_commitment();
    if tx_outputs.account_delta_commitment != pre_fee_delta_commitment {
        return Err(TransactionExecutorError::InconsistentAccountDeltaCommitment {
            in_kernel_commitment: tx_outputs.account_delta_commitment,
            host_commitment: pre_fee_delta_commitment,
        });
    }

    // The full transaction delta is the pre fee delta with the fee asset removed.
    let mut post_fee_account_delta = pre_fee_account_delta;
    post_fee_account_delta
        .vault_mut()
        .remove_asset(Asset::from(tx_outputs.fee))
        .map_err(TransactionExecutorError::RemoveFeeAssetFromDelta)?;

    let initial_account = tx_inputs.account();
    let final_account = &tx_outputs.account;

    if initial_account.id() != final_account.id() {
        return Err(TransactionExecutorError::InconsistentAccountId {
            input_id: initial_account.id(),
            output_id: final_account.id(),
        });
    }

    // make sure nonce delta was computed correctly
    let nonce_delta = final_account.nonce() - initial_account.nonce();
    if nonce_delta != post_fee_account_delta.nonce_delta() {
        return Err(TransactionExecutorError::InconsistentAccountNonceDelta {
            expected: nonce_delta,
            actual: post_fee_account_delta.nonce_delta(),
        });
    }

    // introduce generated signatures into the witness inputs
    advice_inputs.map.extend(generated_signatures);

    Ok(ExecutedTransaction::new(
        tx_inputs,
        tx_outputs,
        post_fee_account_delta,
        tx_args,
        accessed_foreign_account_code,
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
                Some(TransactionKernelError::InsufficientFee { account_balance, tx_fee }) => {
                    TransactionExecutorError::InsufficientFee {
                        account_balance: *account_balance,
                        tx_fee: *tx_fee,
                    }
                },
                _ => TransactionExecutorError::TransactionProgramExecutionFailed(exec_err),
            }
        },
        _ => TransactionExecutorError::TransactionProgramExecutionFailed(exec_err),
    }
}
