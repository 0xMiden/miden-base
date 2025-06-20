#[cfg(feature = "async")]
use alloc::boxed::Box;
use alloc::{borrow::ToOwned, collections::BTreeSet, rc::Rc, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::{Account, AccountId},
    assembly::{Assembler, SourceManager},
    block::{BlockHeader, BlockNumber},
    note::Note,
    transaction::{
        ExecutedTransaction, InputNote, InputNotes, PartialBlockchain, TransactionInputs,
        TransactionParams,
    },
};
use miden_tx::{
    DataStore, DataStoreError, TransactionExecutor, TransactionExecutorError, TransactionMastStore,
    auth::{BasicAuthenticator, TransactionAuthenticator},
};
use rand_chacha::ChaCha20Rng;
use vm_processor::{
    AdviceInputs, Digest, ExecutionError, MastForest, MastForestStore, Process, Word,
};
use winter_maybe_async::*;

use crate::{MockHost, executor::CodeExecutor, tx_context::builder::MockAuthenticator};

// TRANSACTION CONTEXT
// ================================================================================================

/// Represents all needed data for executing a transaction, or arbitrary code.
///
/// It implements [`DataStore`], so transactions may be executed with
/// [TransactionExecutor](miden_tx::TransactionExecutor)
pub struct TransactionContext {
    pub(super) expected_output_notes: Vec<Note>,
    pub(super) tx_params: TransactionParams,
    pub(super) tx_inputs: TransactionInputs,
    pub(super) mast_store: TransactionMastStore,
    pub(super) advice_inputs: AdviceInputs,
    pub(super) authenticator: Option<MockAuthenticator>,
    pub(super) source_manager: Arc<dyn SourceManager>,
}

impl TransactionContext {
    /// Executes arbitrary code within the context of a mocked transaction environment and returns
    /// the resulting [Process].
    ///
    /// The code is compiled with the assembler attached to this context and executed with advice
    /// inputs constructed from the data stored in the context. The program is run on a [MockHost]
    /// which is loaded with the procedures exposed by the transaction kernel, and also individual
    /// kernel functions (not normally exposed).
    ///
    /// To improve the error message quality, convert the returned [`ExecutionError`] into a
    /// [`Report`](miden_objects::assembly::diagnostics::Report).
    ///
    /// # Errors
    ///
    /// Returns an error if the assembly or execution of the provided code fails.
    pub fn execute_code_with_assembler(
        &self,
        code: &str,
        assembler: Assembler,
    ) -> Result<Process, ExecutionError> {
        let (stack_inputs, advice_inputs) = TransactionKernel::prepare_inputs(
            &self.tx_inputs,
            &self.tx_params,
            Some(self.advice_inputs.clone()),
        )
        .unwrap();

        let test_lib = TransactionKernel::kernel_as_library();

        let source_manager = assembler.source_manager();

        // Virtual file name should be unique.
        let virtual_source_file = source_manager.load("_tx_context_code", code.to_owned());

        let program = assembler
            .with_debug_mode(true)
            .assemble_program(virtual_source_file)
            .expect("compilation of the provided code failed");

        let mast_store = Rc::new(TransactionMastStore::new());

        mast_store.insert(program.mast_forest().clone());
        mast_store.insert(test_lib.mast_forest().clone());
        mast_store.load_account_code(self.account().code());
        for acc_inputs in self.tx_params.foreign_account_inputs() {
            mast_store.load_account_code(acc_inputs.code());
        }

        CodeExecutor::new(MockHost::new(
            self.tx_inputs.account().into(),
            advice_inputs,
            mast_store,
            self.tx_params.foreign_account_code_commitments(),
        ))
        .stack_inputs(stack_inputs)
        .execute_program(program, source_manager)
    }

    /// Executes arbitrary code with a testing assembler ([TransactionKernel::testing_assembler()]).
    ///
    /// For more information, see the docs for [TransactionContext::execute_code_with_assembler()].
    pub fn execute_code(&self, code: &str) -> Result<Process, ExecutionError> {
        let assembler = TransactionKernel::testing_assembler();
        self.execute_code_with_assembler(code, assembler)
    }

    /// Executes the transaction through a [TransactionExecutor]
    #[allow(clippy::arc_with_non_send_sync)]
    #[maybe_async]
    pub fn execute(self) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let account_id = self.account().id();
        let block_num = self.tx_inputs().block_header().block_num();
        let notes = self.tx_inputs().input_notes().clone();
        let tx_params = self.tx_params().clone();

        let authenticator = self
            .authenticator()
            .cloned()
            .map(|auth| Arc::new(auth) as Arc<dyn TransactionAuthenticator>);

        let source_manager = Arc::clone(&self.source_manager);
        let tx_executor = TransactionExecutor::new(Arc::new(self), authenticator).with_debug_mode();

        maybe_await!(tx_executor.execute_transaction(
            account_id,
            block_num,
            notes,
            tx_params,
            source_manager
        ))
    }

    pub fn account(&self) -> &Account {
        self.tx_inputs.account()
    }

    pub fn expected_output_notes(&self) -> &[Note] {
        &self.expected_output_notes
    }

    pub fn tx_params(&self) -> &TransactionParams {
        &self.tx_params
    }

    pub fn input_notes(&self) -> &InputNotes<InputNote> {
        self.tx_inputs.input_notes()
    }

    pub fn set_tx_params(&mut self, tx_params: TransactionParams) {
        self.tx_params = tx_params;
    }

    pub fn tx_inputs(&self) -> &TransactionInputs {
        &self.tx_inputs
    }

    pub fn authenticator(&self) -> Option<&BasicAuthenticator<ChaCha20Rng>> {
        self.authenticator.as_ref()
    }

    /// Returns the source manager used in the assembler of the transaction context builder.
    pub fn source_manager(&self) -> Arc<dyn SourceManager> {
        Arc::clone(&self.source_manager)
    }
}

#[maybe_async_trait]
impl DataStore for TransactionContext {
    #[maybe_async]
    fn get_transaction_inputs(
        &self,
        account_id: AccountId,
        _ref_blocks: BTreeSet<BlockNumber>,
    ) -> Result<(Account, Option<Word>, BlockHeader, PartialBlockchain), DataStoreError> {
        assert_eq!(account_id, self.account().id());
        let (account, seed, header, mmr, _) = self.tx_inputs.clone().into_parts();

        Ok((account, seed, header, mmr))
    }
}

impl MastForestStore for TransactionContext {
    fn get(&self, procedure_hash: &Digest) -> Option<Arc<MastForest>> {
        self.mast_store.get(procedure_hash)
    }
}
