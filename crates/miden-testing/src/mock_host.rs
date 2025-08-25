use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::{TransactionEvent, TransactionEventError};
use miden_objects::account::{AccountHeader, AccountVaultDelta};
use miden_objects::assembly::debuginfo::SourceManagerSync;
use miden_objects::assembly::{SourceManager, default_source_manager_arc_dyn};
use miden_objects::{Felt, Word};
use miden_processor::{
    AdviceInputs,
    AdviceMutation,
    BaseHost,
    ContextId,
    EventError,
    MastForest,
    MastForestStore,
    ProcessState,
    SyncHost,
};
use miden_tx::{AccountProcedureIndexMap, LinkMap, TransactionMastStore};

// MOCK HOST
// ================================================================================================

/// This is very similar to the TransactionHost in miden-tx. The differences include:
/// - We do not track account delta here.
/// - There is special handling of EMPTY_DIGEST in account procedure index map.
/// - This host uses `MemAdviceProvider` which is instantiated from the passed in advice inputs.
pub struct MockHost {
    acct_procedure_index_map: AccountProcedureIndexMap,
    mast_store: Rc<TransactionMastStore>,
    source_manager: Arc<dyn SourceManagerSync>,
}

impl MockHost {
    /// Returns a new [MockHost] instance with the provided
    /// [AdviceInputs](miden_processor::AdviceInputs).
    pub fn new(
        account: AccountHeader,
        advice_inputs: &AdviceInputs,
        mast_store: Rc<TransactionMastStore>,
        foreign_code_commitments: BTreeSet<Word>,
    ) -> Self {
        Self::new_with_source_manager(
            account,
            advice_inputs,
            mast_store,
            foreign_code_commitments,
            default_source_manager_arc_dyn(),
        )
    }

    /// Returns a new [MockHost] instance with the provided
    /// [AdviceInputs](vm_processor::AdviceInputs) and [`SourceManagerSync`] implementation.
    pub fn new_with_source_manager(
        account: AccountHeader,
        advice_inputs: &AdviceInputs,
        mast_store: Rc<TransactionMastStore>,
        mut foreign_code_commitments: BTreeSet<Word>,
        source_manager: Arc<dyn SourceManagerSync>,
    ) -> Self {
        foreign_code_commitments.insert(account.code_commitment());
        let proc_index_map = AccountProcedureIndexMap::new(foreign_code_commitments, advice_inputs);

        Self {
            acct_procedure_index_map: proc_index_map.unwrap(),
            mast_store,
            source_manager,
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
        Ok(vec![AdviceMutation::extend_stack([Felt::from(proc_idx)])])
    }
}

impl BaseHost for MockHost {
    fn get_mast_forest(&self, node_digest: &Word) -> Option<Arc<MastForest>> {
        self.mast_store.get(node_digest)
    }

    fn get_label_and_source_file(
        &self,
        location: &miden_objects::assembly::debuginfo::Location,
    ) -> (
        miden_objects::assembly::debuginfo::SourceSpan,
        Option<Arc<miden_objects::assembly::SourceFile>>,
    ) {
        let maybe_file = self.source_manager.get_by_uri(location.uri());
        let span = self.source_manager.location_to_span(location.clone()).unwrap_or_default();
        (span, maybe_file)
    }
}

impl SyncHost for MockHost {
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
