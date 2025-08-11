use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionEvent;
use miden_objects::Word;
use miden_objects::account::{AccountDelta, PartialAccount};
use miden_objects::assembly::debuginfo::Location;
use miden_objects::assembly::{SourceFile, SourceSpan};
use miden_objects::transaction::{InputNote, InputNotes, OutputNote};
use vm_processor::{
    AdviceMutation,
    BaseHost,
    EventError,
    MastForest,
    MastForestStore,
    ProcessState,
    SyncHost,
};

use crate::AccountProcedureIndexMap;
use crate::host::{
    ScriptMastForestStore,
    TransactionBaseHost,
    TransactionEventData,
    TransactionEventHandling,
    TransactionProgress,
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

impl<STORE> BaseHost for TransactionProverHost<'_, STORE>
where
    STORE: MastForestStore,
{
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn get_label_and_source_file(
        &self,
        _location: &Location,
    ) -> (SourceSpan, Option<Arc<SourceFile>>) {
        // For the prover, we assume that the transaction witness is a successfully executed
        // transaction and so there should be no need to provide the actual source manager, as it
        // is only used to improve error message quality which we shouldn't run into here.
        (SourceSpan::UNKNOWN, None)
    }
}

impl<STORE> SyncHost for TransactionProverHost<'_, STORE>
where
    STORE: MastForestStore,
{
    fn on_event(
        &mut self,
        process: &ProcessState,
        event_id: u32,
    ) -> Result<Vec<AdviceMutation>, EventError> {
        let transaction_event = TransactionEvent::try_from(event_id).map_err(Box::new)?;

        match self.base_host.handle_event(process, transaction_event)? {
            TransactionEventHandling::Unhandled(event_data) => {
                // The base host should have handled this event since the signature should be
                // present in the advice map. We match on the event_data here so that if a new
                // variant is added to the enum, this fails compilation and we can adapt
                // accordingly.
                let TransactionEventData::AuthRequest { .. } = event_data;
                Err(EventError::from("base host should have handled auth request event"))
            },
            TransactionEventHandling::Handled(mutations) => Ok(mutations),
        }
    }
}
