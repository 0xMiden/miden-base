use alloc::vec::Vec;

use miden_lib::note::well_known_note::WellKnownNote;
use miden_objects::account::AccountId;
use miden_objects::block::BlockNumber;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs};

use super::{FailedNote, NoteConsumptionInfo, TransactionExecutor};
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
    /// account by executing the transaction with varying combination of notes.
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
        // Ensure well-known notes are ordered first.
        let mut notes = input_notes.into_vec();
        notes.sort_by_key(|note| WellKnownNote::from_note(note.note()).is_none());

        // Attempt to find an executable set of notes.
        self.find_executable_notes_by_elimination(target_account_id, block_ref, notes, tx_args)
            .await
    }

    /// Finds a set of executable notes and eliminates failed notes from the list in the process.
    ///
    /// The result contains some combination of the input notes partitioned by whether they
    /// succeeded or failed to execute.
    async fn find_executable_notes_by_elimination(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        notes: Vec<InputNote>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        let mut candidate_notes = notes;
        let mut failed_notes = Vec::new();

        // Attempt to execute notes in a loop. Reduce the set of notes based on failures until
        // either a set of notes executes without failure or the set of notes cannot be
        // further reduced.
        loop {
            // Execute the candidate notes.
            match self
                .0
                .try_execute_notes(
                    target_account_id,
                    block_ref,
                    InputNotes::<InputNote>::new_unchecked(candidate_notes.clone()),
                    &tx_args,
                )
                .await?
            {
                // A note failed to execute.
                Some((failed_note_index, error)) => {
                    // SAFETY: Failed note index is in bounds of the candidate notes.
                    let failed_note = candidate_notes.remove(failed_note_index).into_note();
                    failed_notes.push(FailedNote::new(failed_note, error));

                    // End if there are no more candidates.
                    if candidate_notes.is_empty() {
                        return Ok(NoteConsumptionInfo::new(Vec::new(), failed_notes));
                    }
                    // Continue and process next set of candidates.
                },
                None => {
                    // A full set of successful notes has been found.
                    let successful =
                        candidate_notes.into_iter().map(InputNote::into_note).collect::<Vec<_>>();
                    return Ok(NoteConsumptionInfo::new(successful, failed_notes));
                },
            }
        }
    }
}
