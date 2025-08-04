use alloc::{borrow::ToOwned, collections::BTreeSet, rc::Rc, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::{Account, AccountId},
    assembly::{
        Assembler, SourceManager,
        debuginfo::{SourceLanguage, Uri},
    },
    block::{BlockHeader, BlockNumber},
    note::Note,
    transaction::{
        ExecutedTransaction, InputNote, InputNotes, PartialBlockchain, TransactionArgs,
        TransactionInputs,
    },
};
use miden_tx::{
    DataStore, DataStoreError, TransactionExecutor, TransactionExecutorError, TransactionMastStore,
    auth::BasicAuthenticator,
};
use rand_chacha::ChaCha20Rng;
use vm_processor::{
    AdviceInputs, AsyncHostFuture, ExecutionError, MastForest, MastForestStore, Process, Word,
};

use crate::{MockHost, executor::CodeExecutor, tx_context::builder::MockAuthenticator};

// TRANSACTION CONTEXT
// ================================================================================================

/// Represents all needed data for executing a transaction, or arbitrary code.
///
/// It implements [`DataStore`], so transactions may be executed with
/// [TransactionExecutor](miden_tx::TransactionExecutor)
pub struct TransactionContext {
    pub(super) expected_output_notes: Vec<Note>,
    pub(super) tx_args: TransactionArgs,
    pub(super) tx_inputs: TransactionInputs,
    pub(super) mast_store: TransactionMastStore,
    pub(super) advice_inputs: AdviceInputs,
    pub(super) authenticator: Option<MockAuthenticator>,
    pub(super) source_manager: Arc<dyn SourceManager + Send + Sync>,
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
    ///
    /// # Panics
    ///
    /// - If the provided `code` is not a valid program.
    pub fn execute_code_with_assembler(
        &self,
        code: &str,
        assembler: Assembler,
    ) -> Result<Process, ExecutionError> {
        let (stack_inputs, advice_inputs) = TransactionKernel::prepare_inputs(
            &self.tx_inputs,
            &self.tx_args,
            Some(self.advice_inputs.clone()),
        )
        .expect("error initializing transaction inputs");

        let test_lib = TransactionKernel::kernel_as_library();

        let source_manager =
            alloc::sync::Arc::new(miden_objects::assembly::DefaultSourceManager::default())
                as alloc::sync::Arc<
                    dyn miden_objects::assembly::SourceManager + Send + Sync + 'static,
                >;

        // TODO: Load source into host-owned source manager.
        // Virtual file name should be unique.
        let virtual_source_file = source_manager.load(
            SourceLanguage::Masm,
            Uri::new("_tx_context_code"),
            code.to_owned(),
        );

        let program = assembler
            .with_debug_mode(true)
            .assemble_program(virtual_source_file)
            .expect("code was not well formed");

        let mast_store = Rc::new(TransactionMastStore::new());

        mast_store.insert(program.mast_forest().clone());
        mast_store.insert(test_lib.mast_forest().clone());
        mast_store.load_account_code(self.account().code());
        for acc_inputs in self.tx_args.foreign_account_inputs() {
            mast_store.load_account_code(acc_inputs.code());
        }

        let advice_inputs = advice_inputs.into_advice_inputs();
        CodeExecutor::new(MockHost::new(
            self.tx_inputs.account().into(),
            &advice_inputs,
            mast_store,
            self.tx_args.to_foreign_account_code_commitments(),
        ))
        .stack_inputs(stack_inputs)
        .extend_advice_inputs(advice_inputs)
        .execute_program(program)
    }

    /// Executes arbitrary code with a testing assembler ([TransactionKernel::testing_assembler()]).
    ///
    /// For more information, see the docs for [TransactionContext::execute_code_with_assembler()].
    pub fn execute_code(&self, code: &str) -> Result<Process, ExecutionError> {
        let assembler = TransactionKernel::testing_assembler();
        self.execute_code_with_assembler(code, assembler)
    }

    /// Executes the transaction through a [TransactionExecutor]
    pub async fn execute(self) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let account_id = self.account().id();
        let block_num = self.tx_inputs().block_header().block_num();
        let notes = self.tx_inputs().input_notes().clone();
        let tx_args = self.tx_args().clone();
        let authenticator = self.authenticator();

        let source_manager = Arc::clone(&self.source_manager);
        let tx_executor = TransactionExecutor::new(&self, authenticator).with_debug_mode();

        tx_executor
            .execute_transaction(account_id, block_num, notes, tx_args, source_manager)
            .await
    }

    /// Executes the transaction through a [TransactionExecutor]
    ///
    /// TODO: This is a temporary workaround to avoid having to update each test to use tokio::test.
    /// Eventually we should get rid of this method and use tokio::test + execute, but for the POC
    /// stage this is easier.
    pub fn execute_blocking(self) -> Result<ExecutedTransaction, TransactionExecutorError> {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(self.execute())
    }

    pub fn account(&self) -> &Account {
        self.tx_inputs.account()
    }

    pub fn expected_output_notes(&self) -> &[Note] {
        &self.expected_output_notes
    }

    pub fn tx_args(&self) -> &TransactionArgs {
        &self.tx_args
    }

    pub fn input_notes(&self) -> &InputNotes<InputNote> {
        self.tx_inputs.input_notes()
    }

    pub fn set_tx_args(&mut self, tx_args: TransactionArgs) {
        self.tx_args = tx_args;
    }

    pub fn tx_inputs(&self) -> &TransactionInputs {
        &self.tx_inputs
    }

    pub fn authenticator(&self) -> Option<&BasicAuthenticator<ChaCha20Rng>> {
        self.authenticator.as_ref()
    }

    /// Returns the source manager used in the assembler of the transaction context builder.
    pub fn source_manager(&self) -> Arc<dyn SourceManager + Send + Sync> {
        Arc::clone(&self.source_manager)
    }
}

impl DataStore for TransactionContext {
    fn get_transaction_inputs(
        &self,
        account_id: AccountId,
        _ref_blocks: BTreeSet<BlockNumber>,
    ) -> impl AsyncHostFuture<
        Result<(Account, Option<Word>, BlockHeader, PartialBlockchain), DataStoreError>,
    > {
        assert_eq!(account_id, self.account().id());
        let (account, seed, header, mmr, _) = self.tx_inputs.clone().into_parts();
        async move { Ok((account, seed, header, mmr)) }
    }
}

impl MastForestStore for TransactionContext {
    fn get(&self, procedure_hash: &Word) -> Option<Arc<MastForest>> {
        self.mast_store.get(procedure_hash)
    }
}
