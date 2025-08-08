use alloc::sync::Arc;

use miden_objects::account::AccountId;
use miden_objects::assembly::SourceManager;
use miden_objects::block::BlockNumber;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs};

use super::{NoteConsumptionInfo, TransactionExecutor};
use crate::auth::TransactionAuthenticator;
use crate::{DataStore, TransactionExecutorError};

/// This struct performs input notes check against provided target account.
///
/// The check is performed using the [NoteConsumptionChecker::check_notes_consumability] procedure.
/// Essentially runs the transaction to make sure that provided input notes could be consumed by the
/// account.
pub struct NoteConsumptionChecker<'a, STORE, AUTH>(&'a TransactionExecutor<'a, 'a, STORE, AUTH>);

impl<'a, STORE, AUTH> NoteConsumptionChecker<'a, STORE, AUTH>
where
    STORE: DataStore + Sync,
    AUTH: TransactionAuthenticator + Sync,
{
    /// Creates a new [`NoteConsumptionChecker`] instance with the given transaction executor.
    pub fn new(tx_executor: &'a TransactionExecutor<'a, 'a, STORE, AUTH>) -> Self {
        NoteConsumptionChecker(tx_executor)
    }

    /// Checks whether the provided input notes could be consumed by the provided account by
    /// executing the transaction.
    pub async fn check_notes_consumability(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        source_manager: Arc<dyn SourceManager + Sync + Send>,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        self.0
            .try_execute_notes(target_account_id, block_ref, input_notes, tx_args, source_manager)
            .await
    }
}
