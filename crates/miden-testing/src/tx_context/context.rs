use alloc::borrow::ToOwned;
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::{Account, AccountId};
use miden_objects::assembly::debuginfo::{SourceLanguage, Uri};
use miden_objects::assembly::{SourceManager, SourceManagerSync};
use miden_objects::block::{BlockHeader, BlockNumber};
use miden_objects::note::Note;
use miden_objects::transaction::{
    ExecutedTransaction,
    InputNote,
    InputNotes,
    PartialBlockchain,
    TransactionArgs,
    TransactionInputs,
};
use miden_processor::{
    AdviceInputs,
    ExecutionError,
    FutureMaybeSend,
    MastForest,
    MastForestStore,
    Process,
    Word,
};
use miden_tx::auth::BasicAuthenticator;
use miden_tx::{
    DataStore,
    DataStoreError,
    TransactionExecutor,
    TransactionExecutorError,
    TransactionMastStore,
};
use rand_chacha::ChaCha20Rng;

use crate::MockHost;
use crate::executor::CodeExecutor;
use crate::tx_context::builder::MockAuthenticator;

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
    pub(super) source_manager: Arc<dyn SourceManagerSync>,
}

impl TransactionContext {
    /// Executes arbitrary code within the context of a mocked transaction environment and returns
    /// the resulting [Process].
    ///
    /// The code is compiled with the assembler returned by
    /// [`TransactionKernel::with_mock_libraries`] and executed with advice inputs constructed from
    /// the data stored in the context. The program is run on a [`MockHost`] which is loaded with
    /// the procedures exposed by the transaction kernel, and also individual kernel functions (not
    /// normally exposed).
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
    ///
    /// # Note
    ///
    /// The `Assembler` passed _must_ use the same `SourceManager` as passed to
    /// `TransactionContext`, otherwise errors will only be available in a truncated
    /// form. See [issue #1779 in miden-base](<https://github.com/0xMiden/miden-base/issues/1779>).
    pub fn execute_code(&self, code: &str) -> Result<Process, ExecutionError> {
        let (stack_inputs, advice_inputs) = TransactionKernel::prepare_inputs(
            &self.tx_inputs,
            &self.tx_args,
            Some(self.advice_inputs.clone()),
        )
        .expect("error initializing transaction inputs");

        // Virtual file name should be unique.
        let virtual_source_file = self.source_manager.load(
            SourceLanguage::Masm,
            Uri::new("_tx_context_code"),
            code.to_owned(),
        );

        let assembler = TransactionKernel::with_mock_libraries(self.source_manager.clone())
            .with_debug_mode(true);
        let program = assembler
            .with_debug_mode(true)
            .assemble_program(virtual_source_file)
            .expect("code was not well formed");

        let mast_store = Rc::new(TransactionMastStore::new());

        mast_store.insert(program.mast_forest().clone());
        mast_store.insert(TransactionKernel::kernel_as_library().mast_forest().clone());
        mast_store.load_account_code(self.account().code());
        for acc_inputs in self.tx_args.foreign_account_inputs() {
            mast_store.load_account_code(acc_inputs.code());
        }

        let advice_inputs = advice_inputs.into_advice_inputs();
        CodeExecutor::new(
            MockHost::new(
                self.tx_inputs.account().into(),
                &advice_inputs,
                mast_store,
                self.tx_args.to_foreign_account_code_commitments(),
            )
            .with_source_manager(self.source_manager()),
        )
        .stack_inputs(stack_inputs)
        .extend_advice_inputs(advice_inputs)
        .execute_program(program)
    }

    /// Executes the transaction through a [TransactionExecutor]
    pub async fn execute(self) -> Result<ExecutedTransaction, TransactionExecutorError> {
        let account_id = self.account().id();
        let block_num = self.tx_inputs().block_header().block_num();
        let notes = self.tx_inputs().input_notes().clone();
        let tx_args = self.tx_args().clone();

        let mut tx_executor = TransactionExecutor::new(&self)
            .with_source_manager(self.source_manager.clone())
            .with_debug_mode();
        if let Some(authenticator) = self.authenticator() {
            tx_executor = tx_executor.with_authenticator(authenticator);
        }

        tx_executor.execute_transaction(account_id, block_num, notes, tx_args).await
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
    pub fn source_manager(&self) -> Arc<dyn SourceManagerSync> {
        Arc::clone(&self.source_manager)
    }
}

impl DataStore for TransactionContext {
    fn get_transaction_inputs(
        &self,
        account_id: AccountId,
        _ref_blocks: BTreeSet<BlockNumber>,
    ) -> impl FutureMaybeSend<
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
