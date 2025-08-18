use alloc::vec::Vec;

use miden_objects::account::AccountId;
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

    /// Checks whether some set of the provided input notes could be consumed by the provided
    /// account by executing the transaction using an iterative elimination strategy.
    ///
    /// This function attempts to find the maximum set of notes that can be successfully executed
    /// together by the target account.
    ///
    /// If some notes succeed but others fail, the failed notes are removed from the candidate set
    /// and the remaining notes (successful + unattempted) are retried in the next iteration. This
    /// process continues until either all remaining notes succeed or no notes can be successfully
    /// executed
    ///
    /// # Example Execution Flow
    ///
    /// Given notes A, B, C, D, E:
    /// - Try [A, B, C, D, E] → A, B succeed, C fails → Remove C, try again.
    /// - Try [A, B, D, E] → A, B, D succeed, E fails → Remove E, try again.
    /// - Try [A, B, D] → All succeed → Return successful=[A, B, D], failed=[C, E].
    ///
    /// # Returns
    ///
    /// Returns [`NoteConsumptionInfo`] containing:
    /// - `successful`: Notes that can be consumed together by the account.
    /// - `failed`: Notes that failed during execution attempts.
    pub async fn check_notes_consumability(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        let mut candidate_notes = input_notes.into_vec();
        let mut failed_notes = Vec::new();

        // Attempt to execute notes in a loop. Reduce the set of notes based on failures until
        // either a set of notes executes without failure or the set of notes cannot be
        // further reduced.
        loop {
            // Execute the candidate notes.
            let execution_result = self
                .0
                .try_execute_notes(
                    target_account_id,
                    block_ref,
                    InputNotes::<InputNote>::new_unchecked(candidate_notes.clone()),
                    &tx_args,
                )
                .await?;
            let successful_count = execution_result.successful.len();

            if successful_count == candidate_notes.len() {
                // A full set of successful notes has been found.
                return Ok(NoteConsumptionInfo::new(execution_result.successful, failed_notes));
            } else if successful_count == 0 {
                // All notes failed, make no further attempts.
                failed_notes.extend(execution_result.failed);
                return Ok(NoteConsumptionInfo::new(Vec::new(), failed_notes));
            } else {
                // Some notes succeeded and some failed.

                // Remove the failed notes from the candidate list.
                let failed_range =
                    successful_count..successful_count + execution_result.failed.len();
                candidate_notes.drain(failed_range);

                // Record the failed notes.
                failed_notes.extend(execution_result.failed);

                // Continue with the remaining candidate notes.
            }
        }
    }
}
