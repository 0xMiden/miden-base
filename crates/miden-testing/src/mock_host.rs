use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::sync::Arc;

use miden_lib::transaction::{TransactionEvent, TransactionEventError};
use miden_objects::account::{AccountHeader, AccountVaultDelta};
use miden_objects::{Felt, Word};
use miden_tx::{AccountProcedureIndexMap, LinkMap, TransactionMastStore};
use vm_processor::{
    AdviceInputs,
    BaseHost,
    ContextId,
    ErrorContext,
    ExecutionError,
    MastForest,
    MastForestStore,
    ProcessState,
    SyncHost,
};

// MOCK HOST
// ================================================================================================

/// This is very similar to the TransactionHost in miden-tx. The differences include:
/// - We do not track account delta here.
/// - There is special handling of EMPTY_DIGEST in account procedure index map.
/// - This host uses `MemAdviceProvider` which is instantiated from the passed in advice inputs.
pub struct MockHost {
    acct_procedure_index_map: AccountProcedureIndexMap,
    mast_store: Rc<TransactionMastStore>,
}

impl MockHost {
    /// Returns a new [MockHost] instance with the provided
    /// [AdviceInputs](vm_processor::AdviceInputs).
    pub fn new(
        account: AccountHeader,
        advice_inputs: &AdviceInputs,
        mast_store: Rc<TransactionMastStore>,
        mut foreign_code_commitments: BTreeSet<Word>,
    ) -> Self {
        foreign_code_commitments.insert(account.code_commitment());
        let proc_index_map = AccountProcedureIndexMap::new(foreign_code_commitments, advice_inputs);

        Self {
            acct_procedure_index_map: proc_index_map.unwrap(),
            mast_store,
        }
    }

    /// Consumes `self` and returns the advice provider and account vault delta.
    pub fn into_parts(self) -> AccountVaultDelta {
        AccountVaultDelta::default()
    }

    // EVENT HANDLERS
    // --------------------------------------------------------------------------------------------

    fn on_push_account_procedure_index(
        &mut self,
        process: &ProcessState,
    ) -> Result<Vec<AdviceMutation>, EventError> {
        let proc_idx = self.acct_procedure_index_map.get_proc_index(process).map_err(Box::new)?;
        Ok(vec![AdviceMutation::ExtendStack { values: vec![Felt::from(proc_idx)] }])
    }
}

impl BaseHost for MockHost {
    fn get_label_and_source_file(
        &self,
        location: &miden_objects::assembly::debuginfo::Location,
    ) -> (
        miden_objects::assembly::debuginfo::SourceSpan,
        Option<Arc<miden_objects::assembly::SourceFile>>,
    ) {
        // TODO: Replace with proper call to source manager once the host owns it.
        let stub_source_manager = miden_objects::assembly::DefaultSourceManager::default();
        let maybe_file = stub_source_manager.get_by_uri(location.uri());
        let span = stub_source_manager.location_to_span(location.clone()).unwrap_or_default();
        (span, maybe_file)
    }
}

impl SyncHost for MockHost {
    fn get_mast_forest(&self, node_digest: &Word) -> Option<Arc<MastForest>> {
        self.mast_store.get(node_digest)
    }

    fn on_event(
        &mut self,
        process: &ProcessState,
        event_id: u32,
    ) -> Result<Vec<AdviceMutation>, EventError> {
        let event = TransactionEvent::try_from(event_id).map_err(Box::new)?;

        if process.ctx() != ContextId::root() {
            return Err(Box::new(TransactionEventError::NotRootContext(event_id)));
        }

        let advice_mutations = match event {
            TransactionEvent::AccountPushProcedureIndex => {
                self.on_push_account_procedure_index(process)
            },
            TransactionEvent::LinkMapSetEvent => LinkMap::handle_set_event(process),
            TransactionEvent::LinkMapGetEvent => LinkMap::handle_get_event(process),
            _ => Ok(Vec::new()),
        }?;

        Ok(advice_mutations)
    }
}
