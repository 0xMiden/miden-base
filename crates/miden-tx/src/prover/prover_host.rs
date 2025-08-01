use alloc::{boxed::Box, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionEvent;
use miden_objects::{
    Word,
    account::{AccountDelta, PartialAccount},
    transaction::{InputNote, InputNotes, OutputNote},
};
use vm_processor::{
    BaseHost, ErrorContext, ExecutionError, MastForest, MastForestStore, ProcessState, SyncHost,
};

use crate::{
    AccountProcedureIndexMap,
    host::{ScriptMastForestStore, TransactionBaseHost, TransactionProgress},
};

/// The transaction prover host is responsible for handling [`SyncHost`] requests made by the
/// transaction kernel during proving.
pub struct TransactionProverHost<'store, STORE>
where
    STORE: MastForestStore,
{
    /// The underlying base transaction host.
    base_host: TransactionBaseHost<'store, STORE>,
}

impl<'store, STORE> TransactionProverHost<'store, STORE>
where
    STORE: MastForestStore,
{
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionProverHost`] instance from the provided inputs.
    pub fn new(
        account: &PartialAccount,
        input_notes: InputNotes<InputNote>,
        mast_store: &'store STORE,
        scripts_mast_store: ScriptMastForestStore,
        acct_procedure_index_map: AccountProcedureIndexMap,
    ) -> Self {
        let base_host = TransactionBaseHost::new(
            account,
            input_notes,
            mast_store,
            scripts_mast_store,
            acct_procedure_index_map,
        );

        Self { base_host }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        self.base_host.tx_progress()
    }

    /// Consumes `self` and returns the account delta, output notes and transaction progress.
    pub fn into_parts(self) -> (AccountDelta, Vec<OutputNote>, TransactionProgress) {
        self.base_host.into_parts()
    }
}

// HOST IMPLEMENTATION
// ================================================================================================

impl<STORE> BaseHost for TransactionProverHost<'_, STORE> where STORE: MastForestStore {}

impl<STORE> SyncHost for TransactionProverHost<'_, STORE>
where
    STORE: MastForestStore,
{
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn on_event(
        &mut self,
        process: &mut ProcessState,
        event_id: u32,
        err_ctx: &impl ErrorContext,
    ) -> Result<(), ExecutionError> {
        let transaction_event = TransactionEvent::try_from(event_id)
            .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;

        self.base_host.handle_event(process, transaction_event, err_ctx)
    }
}
