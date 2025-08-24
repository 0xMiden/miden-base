use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use miden_lib::note::well_known_note::WellKnownNote;
use miden_lib::transaction::TransactionKernel;
use miden_objects::account::AccountId;
use miden_objects::block::BlockNumber;
use miden_objects::note::Note;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs};
use miden_processor::fast::FastProcessor;

use super::TransactionExecutor;
use crate::auth::TransactionAuthenticator;
use crate::{DataStore, TransactionExecutorError};

// NOTE CONSUMPTION INFO
// ================================================================================================

/// Represents a failed note consumption.
#[derive(Debug)]
#[non_exhaustive]
pub struct FailedNote {
    pub note: Note,
    pub error: TransactionExecutorError,
}

impl FailedNote {
    /// Constructs a new `FailedNote`.
    pub fn new(note: Note, error: TransactionExecutorError) -> Self {
        Self { note, error }
    }
}

/// Contains information about the successful and failed consumption of notes.
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct NoteConsumptionInfo {
    pub successful: Vec<Note>,
    pub failed: Vec<FailedNote>,
}

impl NoteConsumptionInfo {
    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful notes.
    pub fn new_successful(successful: Vec<Note>) -> Self {
        Self { successful, ..Default::default() }
    }

    /// Creates a new [`NoteConsumptionInfo`] instance with the given successful and failed notes.
    pub fn new(successful: Vec<Note>, failed: Vec<FailedNote>) -> Self {
        Self { successful, failed }
    }
}

// TRANSACTION EXECUTION ATTEMPT
// ================================================================================================

/// The result of trying to execute a transaction.
pub enum TransactionExecutionAttempt {
    Successful,
    NoteFailed {
        failed_note_index: usize,
        error: TransactionExecutorError,
    },
    EpilogueFailed,
}

// NOTE CONSUMPTION CHECKER
// ================================================================================================

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
        notes.sort_unstable_by_key(|note| WellKnownNote::from_note(note.note()).is_none());
        let notes = InputNotes::<InputNote>::new_unchecked(notes);

        // Attempt to find an executable set of notes.
        self.find_executable_notes_by_elimination(target_account_id, block_ref, notes, tx_args)
            .await
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Finds a set of executable notes and eliminates failed notes from the list in the process.
    ///
    /// The result contains some combination of the input notes partitioned by whether they
    /// succeeded or failed to execute.
    async fn find_executable_notes_by_elimination(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        let mut candidate_notes = notes.clone().into_vec();
        let mut failed_notes = Vec::new();

        // Attempt to execute notes in a loop. Reduce the set of notes based on failures until
        // either a set of notes executes without failure or the set of notes cannot be
        // further reduced.
        loop {
            // Execute the candidate notes.
            match self
                .try_execute_notes(
                    target_account_id,
                    block_ref,
                    InputNotes::<InputNote>::new_unchecked(candidate_notes.clone()),
                    &tx_args,
                )
                .await?
            {
                TransactionExecutionAttempt::Successful => {
                    // A full set of successful notes has been found.
                    let successful =
                        candidate_notes.into_iter().map(InputNote::into_note).collect::<Vec<_>>();
                    return Ok(NoteConsumptionInfo::new(successful, failed_notes));
                },
                TransactionExecutionAttempt::NoteFailed { failed_note_index, error } => {
                    // SAFETY: Failed note index is in bounds of the candidate notes.
                    let failed_note = candidate_notes.remove(failed_note_index).into_note();
                    failed_notes.push(FailedNote::new(failed_note, error));

                    // End if there are no more candidates.
                    if candidate_notes.is_empty() {
                        return Ok(NoteConsumptionInfo::new(Vec::new(), failed_notes));
                    }
                    // Continue and process the next set of candidates.
                },
                TransactionExecutionAttempt::EpilogueFailed => {
                    return self
                        .find_largest_executable_combination(
                            target_account_id,
                            block_ref,
                            candidate_notes,
                            failed_notes,
                            &tx_args,
                        )
                        .await;
                },
            }
        }
    }

    /// Finds the largest possible combination of notes that can execute successfully together.
    ///
    /// This method incrementally tries combinations of increasing size (1 note, 2 notes, 3 notes,
    /// etc.) and builds upon previously successful combinations to find the maximum executable
    /// set.
    async fn find_largest_executable_combination(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        candidate_notes: Vec<InputNote>,
        mut failed_notes: Vec<FailedNote>,
        tx_args: &TransactionArgs,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        let mut successful_input_notes: Vec<InputNote> = Vec::new();
        let mut remaining_notes = candidate_notes.clone();

        // Iterate by note count: try 1 note, then 2, then 3, etc.
        for size in 1..=candidate_notes.len() {
            // Can't build a combination of size N without at least N-1 successful notes.
            if successful_input_notes.len() < size - 1 {
                break;
            }

            // Try adding each remaining note to the current successful combination.
            let mut found_successful = None;
            for (idx, note) in remaining_notes.iter().enumerate() {
                let mut test_notes = successful_input_notes.clone();
                test_notes.push(note.clone());

                match self
                    .try_execute_notes(
                        target_account_id,
                        block_ref,
                        InputNotes::<InputNote>::new_unchecked(test_notes.clone()),
                        tx_args,
                    )
                    .await
                {
                    Ok(TransactionExecutionAttempt::Successful) => {
                        successful_input_notes = test_notes;
                        found_successful = Some(idx);
                        break;
                    },
                    _ => {
                        // This combination failed, continue to next.
                    },
                };
            }

            // Remove the successful note for next iteration.
            if let Some(idx) = found_successful {
                remaining_notes.remove(idx);
            }
        }

        // Convert successful InputNotes to Notes.
        let successful =
            successful_input_notes.into_iter().map(InputNote::into_note).collect::<Vec<_>>();

        // Update failed_notes with notes that weren't included in successful combination
        let successful_note_ids = successful.iter().map(|note| note.id()).collect::<BTreeSet<_>>();
        let newly_failed: Vec<_> = candidate_notes
            .into_iter()
            .filter(|input_note| !successful_note_ids.contains(&input_note.note().id()))
            .map(|input_note| {
                FailedNote::new(
                    input_note.into_note(),
                    TransactionExecutorError::DiscardedDuringRetry,
                )
            })
            .collect();
        failed_notes.extend(newly_failed);

        Ok(NoteConsumptionInfo::new(successful, failed_notes))
    }

    /// Validates input notes, transaction inputs, and account inputs before executing the
    /// transaction with specified notes. Keeps track and returns both successfully consumed notes
    /// as well as notes that failed to be consumed.
    ///
    /// The `source_manager` is used to map potential errors back to their source code. To get the
    /// most value out of it, use the source manager from the
    /// [`Assembler`](miden_objects::assembly::Assembler) that assembled the Miden Assembly code
    /// that should be debugged, e.g. account components, note scripts or transaction scripts. If
    /// no error-to-source mapping is desired, a default source manager can be passed, e.g.
    /// [`DefaultSourceManager::default`](miden_objects::assembly::DefaultSourceManager::default).
    ///
    /// # Returns:
    /// - An index into the input notes for the note that failed execution along with the associated
    ///   error.
    ///
    /// # Errors:
    /// Returns an error if:
    /// - If required data can not be fetched from the [`DataStore`].
    /// - If the transaction host can not be created from the provided values.
    /// - If the execution of the provided program fails on the stage other than note execution.
    async fn try_execute_notes(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: &TransactionArgs,
    ) -> Result<TransactionExecutionAttempt, TransactionExecutorError> {
        if notes.is_empty() {
            return Ok(TransactionExecutionAttempt::Successful);
        }

        // TODO: ideally, we should prepare the inputs only once for the while note consumption
        // check (rather than doing this every time when we try to execute some subset of notes),
        // but we currently cannot do this because transaction preparation includes input notes;
        // we should refactor the preparation process to separate input note preparation from the
        // rest, and then we can prepare the rest of the inputs once for the whole check
        let (mut host, _, stack_inputs, advice_inputs) =
            self.0.prepare_transaction(account_id, block_ref, notes, tx_args, None).await?;

        let processor =
            FastProcessor::new_with_advice_inputs(stack_inputs.as_slice(), advice_inputs);
        let result = processor
            .execute(&TransactionKernel::main(), &mut host)
            .await
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed);

        match result {
            Ok(_) => Ok(TransactionExecutionAttempt::Successful),
            Err(error) => {
                let notes = host.tx_progress().note_execution();

                // Empty notes vector means that we didn't process the notes, so an error
                // occurred.
                if notes.is_empty() {
                    return Err(error);
                }

                let ((_failed_note, last_note_interval), success_notes) =
                    notes.split_last().expect("notes vector is not empty because of earlier check");

                // If the interval end of the last note is specified, then an error occurred after
                // notes processing.
                if last_note_interval.end().is_some() {
                    Ok(TransactionExecutionAttempt::EpilogueFailed)
                } else {
                    // Return the index of the failed note.
                    let failed_note_index = success_notes.len();
                    Ok(TransactionExecutionAttempt::NoteFailed { failed_note_index, error })
                }
            },
        }
    }
}
