use alloc::borrow::ToOwned;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::{Account, AccountId, PartialAccount, StorageMapWitness, StorageSlot};
use miden_objects::assembly::debuginfo::{SourceLanguage, Uri};
use miden_objects::assembly::{SourceManager, SourceManagerSync};
use miden_objects::asset::AssetWitness;
use miden_objects::block::{BlockHeader, BlockNumber};
use miden_objects::note::Note;
use miden_objects::transaction::{
    AccountInputs,
    ExecutedTransaction,
    InputNote,
    InputNotes,
    PartialBlockchain,
    TransactionArgs,
    TransactionInputs,
};
use miden_processor::fast::ExecutionOutput;
use miden_processor::{
    AdviceInputs,
    ExecutionError,
    FutureMaybeSend,
    MastForest,
    MastForestStore,
    Word,
};
use miden_tx::auth::{BasicAuthenticator, UnreachableAuth};
use miden_tx::{
    AccountProcedureIndexMap,
    DataStore,
    DataStoreError,
    ScriptMastForestStore,
    TransactionExecutor,
    TransactionExecutorError,
    TransactionExecutorHost,
    TransactionMastStore,
};
use rand_chacha::ChaCha20Rng;

use crate::executor::CodeExecutor;
use crate::mock_host::MockHost;
use crate::tx_context::builder::MockAuthenticator;

// TRANSACTION CONTEXT
// ================================================================================================

/// Represents all needed data for executing a transaction, or arbitrary code.
///
/// It implements [`DataStore`], so transactions may be executed with
/// [TransactionExecutor](miden_tx::TransactionExecutor)
pub struct TransactionContext {
    pub(super) account: Account,
    pub(super) expected_output_notes: Vec<Note>,
    pub(super) foreign_account_inputs: BTreeMap<AccountId, AccountInputs>,
    pub(super) tx_args: TransactionArgs,
    pub(super) tx_inputs: TransactionInputs,
    pub(super) mast_store: TransactionMastStore,
    pub(super) authenticator: Option<MockAuthenticator>,
    pub(super) source_manager: Arc<dyn SourceManagerSync>,
    pub(super) advice_inputs: AdviceInputs,
    pub(super) is_lazy_loading_enabled: bool,
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
    pub fn execute_code(&self, code: &str) -> Result<ExecutionOutput, ExecutionError> {
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

        // Load transaction kernel and the program into the mast forest in self.
        // Note that native and foreign account's code are already loaded by the
        // TransactionContextBuilder.
        self.mast_store.insert(TransactionKernel::library().mast_forest().clone());
        self.mast_store.insert(program.mast_forest().clone());

        let account_procedure_idx_map = AccountProcedureIndexMap::new(
            [self.tx_inputs().account().code()]
                .into_iter()
                .chain(self.tx_args().foreign_account_inputs().iter().map(|inputs| inputs.code())),
        )
        .expect("constructing account procedure index map should work");

        // The ref block is unimportant when using execute_code so we can set it to any value.
        let ref_block = self.tx_inputs().block_header().block_num();

        let exec_host = TransactionExecutorHost::<'_, '_, _, UnreachableAuth>::new(
            &PartialAccount::from(self.account()),
            self.tx_inputs().input_notes().clone(),
            self,
            ScriptMastForestStore::default(),
            account_procedure_idx_map,
            None,
            ref_block,
            self.source_manager(),
        );

        let advice_inputs = advice_inputs.into_advice_inputs();

        let mut mock_host = MockHost::new(exec_host);
        if self.is_lazy_loading_enabled {
            mock_host.enable_lazy_loading()
        }

        CodeExecutor::new(mock_host)
            .stack_inputs(stack_inputs)
            .extend_advice_inputs(advice_inputs)
            .execute_program(program)
    }
}

impl TransactionContext {
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
        &self.account
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
    ) -> impl FutureMaybeSend<Result<(PartialAccount, BlockHeader, PartialBlockchain), DataStoreError>>
    {
        assert_eq!(account_id, self.account().id());
        assert_eq!(account_id, self.tx_inputs.account().id());

        let (partial_account, header, mmr, _) = self.tx_inputs.clone().into_parts();

        async move { Ok((partial_account, header, mmr)) }
    }

    fn get_foreign_account_inputs(
        &self,
        foreign_account_id: AccountId,
        _ref_block: BlockNumber,
    ) -> impl FutureMaybeSend<Result<AccountInputs, DataStoreError>> {
        // Note that we cannot validate that the foreign account inputs are valid for the
        // transaction's reference block.
        async move {
            self.foreign_account_inputs.get(&foreign_account_id).cloned().ok_or_else(|| {
                DataStoreError::other(format!(
                    "failed to find foreign account {foreign_account_id}"
                ))
            })
        }
    }

    fn get_vault_asset_witness(
        &self,
        account_id: AccountId,
        vault_root: Word,
        vault_key: Word,
    ) -> impl FutureMaybeSend<Result<AssetWitness, DataStoreError>> {
        async move {
            if account_id == self.account().id() {
                if self.account().vault().root() != vault_root {
                    return Err(DataStoreError::other(format!(
                        "native account {account_id} has vault root {} but {vault_root} was requested",
                        self.account().vault().root()
                    )));
                }

                Ok(self.account().vault().open(vault_key))
            } else {
                let foreign_account_inputs = self
                    .foreign_account_inputs
                    .iter()
                    .find_map(
                        |(id, account_inputs)| {
                            if account_id == *id { Some(account_inputs) } else { None }
                        },
                    )
                    .ok_or_else(|| {
                        DataStoreError::other(format!(
                            "failed to find foreign account {account_id} in foreign account inputs"
                        ))
                    })?;

                if foreign_account_inputs.account().vault().root() != vault_root {
                    return Err(DataStoreError::other(format!(
                        "foreign account {account_id} has vault root {} but {vault_root} was requested",
                        foreign_account_inputs.account().vault().root()
                    )));
                }

                foreign_account_inputs.account().vault().open(vault_key).map_err(|err| {
                    DataStoreError::other_with_source(
                        format!(
                            "failed to open vault_key {vault_key} in foreign account {account_id}"
                        ),
                        err,
                    )
                })
            }
        }
    }

    fn get_storage_map_witness(
        &self,
        account_id: AccountId,
        map_root: Word,
        map_key: Word,
    ) -> impl FutureMaybeSend<Result<StorageMapWitness, DataStoreError>> {
        async move {
            if account_id == self.account().id() {
                // Iterate the account storage to find the map with the requested root.
                let storage_map = self
                    .account()
                    .storage()
                    .slots()
                    .iter()
                    .find_map(|slot| match slot {
                        StorageSlot::Map(storage_map) if storage_map.root() == map_root => {
                            Some(storage_map)
                        },
                        _ => None,
                    })
                    .ok_or_else(|| {
                        DataStoreError::other(format!(
                            "failed to find storage map with root {map_root} in account storage"
                        ))
                    })?;

                Ok(storage_map.open(&map_key))
            } else {
                let foreign_account_inputs = self
                    .foreign_account_inputs
                    .iter()
                    .find_map(
                        |(id, account_inputs)| {
                            if account_id == *id { Some(account_inputs) } else { None }
                        },
                    )
                    .ok_or_else(|| {
                        DataStoreError::other(format!(
                            "failed to find foreign account {account_id} in foreign account inputs"
                        ))
                    })?;

                let map = foreign_account_inputs
                    .account()
                    .storage()
                    .maps()
                    .find(|map| map.root() == map_root)
                    .ok_or_else(|| {
                        DataStoreError::other(format!(
                            "failed to find storage map with root {map_root} in foreign account {account_id}"
                        ))
                    })?;

                map.open(&map_key).map_err(|err| {
                  DataStoreError::other_with_source(format!(
                        "failed to open {map_key} in storage map of foreign account {account_id}"
                    ), err)
                })
            }
        }
    }
}

impl MastForestStore for TransactionContext {
    fn get(&self, procedure_hash: &Word) -> Option<Arc<MastForest>> {
        self.mast_store.get(procedure_hash)
    }
}
