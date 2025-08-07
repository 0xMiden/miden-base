use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_objects::account::AccountId;
use miden_objects::assembly::SourceManager;
use miden_objects::block::BlockNumber;
use miden_objects::note::Note;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs};
use winter_maybe_async::maybe_await;

use super::TransactionExecutor;
use crate::auth::TransactionAuthenticator;
use crate::errors::NoteConsumptionError;
use crate::{DataStore, TransactionExecutorError};

// NOTE CONSUMPTION INFO
// ================================================================================================

/// Contains information about the successful and failed consumption of notes.
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct NoteConsumptionInfo {
    pub successful: Vec<Note>,
    pub failed: Vec<NoteConsumptionError>,
}

impl NoteConsumptionInfo {
    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful notes.
    pub fn new_successful(successful: Vec<Note>) -> Self {
        Self { successful, ..Default::default() }
    }

    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful and failed notes.
    pub fn new(successful: Vec<Note>, failed: Vec<NoteConsumptionError>) -> Self {
        Self { successful, failed }
    }
}

/// This struct performs input notes check against provided target account.
///
/// The check is performed using the [NoteConsumptionChecker::check_notes_consumability] procedure.
/// Essentially runs the transaction to make sure that provided input notes could be consumed by the
/// account.
pub struct NoteConsumptionChecker<'a, STORE, AUTH>(&'a TransactionExecutor<'a, 'a, STORE, AUTH>);

impl<'a, STORE, AUTH> NoteConsumptionChecker<'a, STORE, AUTH>
where
    STORE: DataStore,
    AUTH: TransactionAuthenticator,
{
    /// Creates a new [`NoteConsumptionChecker`] instance with the given transaction executor.
    pub fn new(tx_executor: &'a TransactionExecutor<'a, 'a, STORE, AUTH>) -> Self {
        NoteConsumptionChecker(tx_executor)
    }

    /// Checks whether the provided input notes could be consumed by the provided account by
    /// executing the transaction.
    pub fn check_notes_consumability(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        maybe_await!(self.0.try_execute_notes(
            target_account_id,
            block_ref,
            input_notes,
            tx_args,
            source_manager
        ))
    }
}
