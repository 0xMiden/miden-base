use alloc::{boxed::Box, collections::BTreeSet, rc::Rc, sync::Arc};

use miden_lib::transaction::{TransactionEvent, TransactionEventError};
use miden_objects::{
    Felt, Word,
    account::{AccountHeader, AccountVaultDelta},
};
use miden_tx::{AccountProcedureIndexMap, LinkMap, TransactionMastStore};
use vm_processor::{
    AdviceInputs, BaseHost, ContextId, ErrorContext, ExecutionError, MastForest, MastForestStore,
    ProcessState, SyncHost,
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
        process: &mut ProcessState,
        err_ctx: &impl ErrorContext,
    ) -> Result<(), ExecutionError> {
        let proc_idx = self
            .acct_procedure_index_map
            .get_proc_index(process)
            .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;
        process.advice_provider_mut().push_stack(Felt::from(proc_idx));
        Ok(())
    }
}

impl BaseHost for MockHost {}

impl SyncHost for MockHost {
    fn get_mast_forest(&self, node_digest: &Word) -> Option<Arc<MastForest>> {
        self.mast_store.get(node_digest)
    }

    fn on_event(
        &mut self,
        process: &mut ProcessState,
        event_id: u32,
        err_ctx: &impl ErrorContext,
    ) -> Result<(), ExecutionError> {
        let event = TransactionEvent::try_from(event_id)
            .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;

        if process.ctx() != ContextId::root() {
            return Err(ExecutionError::event_error(
                Box::new(TransactionEventError::NotRootContext(event_id)),
                err_ctx,
            ));
        }

        match event {
            TransactionEvent::AccountPushProcedureIndex => {
                self.on_push_account_procedure_index(process, err_ctx)
            },
            TransactionEvent::LinkMapSetEvent => LinkMap::handle_set_event(process),
            TransactionEvent::LinkMapGetEvent => LinkMap::handle_get_event(process),
            _ => Ok(()),
        }?;

        Ok(())
    }
}
