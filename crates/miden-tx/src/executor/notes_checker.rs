use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use miden_lib::note::well_known_note::WellKnownNote;
use miden_lib::transaction::TransactionKernel;
use miden_objects::account::AccountId;
use miden_objects::block::BlockNumber;
use miden_objects::note::Note;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs, TransactionInputs};
use miden_processor::fast::FastProcessor;

use super::TransactionExecutor;
use crate::auth::TransactionAuthenticator;
use crate::errors::TransactionCheckerError;
use crate::{DataStore, NoteCheckerError, TransactionExecutorError};

// CONSTANTS
// ================================================================================================

/// Maximum number of notes that can be checked at once.
///
/// Fixed at an amount that should keep each run of note consumption checking to a maximum of ~50ms.
pub const MAX_NUM_CHECKER_NOTES: usize = 20;

// NOTE CONSUMPTION INFO
// ================================================================================================

/// Represents a failed note consumption.
#[derive(Debug)]
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
    /// Because of the runtime complexity involved in this function, a limited range of
    /// [`MAX_NUM_CHECKER_NOTES`] input notes is allowed.
    ///
    /// If some notes succeed and others fail, the failed notes are removed from the candidate set
    /// and the remaining notes (successful + unattempted) are retried in the next iteration. This
    /// process continues until either all remaining notes succeed or no notes can be successfully
    /// executed
    ///
    /// For example, given notes A, B, C, D, E, the execution flow would be as follows:
    /// - Try [A, B, C, D, E] → A, B succeed, C fails → Remove C, try again.
    /// - Try [A, B, D, E] → A, B, D succeed, E fails → Remove E, try again.
    /// - Try [A, B, D] → All succeed → Return successful=[A, B, D], failed=[C, E].
    ///
    /// If a failure occurs at the epilogue phase of the transaction execution, the relevant set of
    /// otherwise-successful notes are retried in various combinations in an attempt to find a
    /// combination that passes the epilogue phase successfully.
    ///
    /// Returns a list of successfully consumed notes and a list of failed notes.
    pub async fn check_notes_consumability(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        mut notes: Vec<Note>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, NoteCheckerError> {
        let num_notes = notes.len();
        if num_notes == 0 || num_notes > MAX_NUM_CHECKER_NOTES {
            return Err(NoteCheckerError::InputNoteCountOutOfRange(num_notes));
        }
        // Ensure well-known notes are ordered first.
        notes.sort_unstable_by_key(|note| WellKnownNote::from_note(note).is_none());

        let notes = InputNotes::from(notes);
        let tx_inputs = self
            .0
            .prepare_transaction_inputs(target_account_id, block_ref, notes, &tx_args)
            .await
            .map_err(NoteCheckerError::TransactionPreparation)?;

        // Attempt to find an executable set of notes.
        self.find_executable_notes_by_elimination(tx_inputs, tx_args).await
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Finds a set of executable notes and eliminates failed notes from the list in the process.
    ///
    /// The result contains some combination of the input notes partitioned by whether they
    /// succeeded or failed to execute.
    async fn find_executable_notes_by_elimination(
        &self,
        mut tx_inputs: TransactionInputs,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, NoteCheckerError> {
        let mut candidate_notes = tx_inputs
            .input_notes()
            .clone()
            .into_iter()
            .map(InputNote::into_note)
            .collect::<Vec<_>>();
        let mut failed_notes = Vec::new();

        // Attempt to execute notes in a loop. Reduce the set of notes based on failures until
        // either a set of notes executes without failure or the set of notes cannot be
        // further reduced.
        loop {
            // Execute the candidate notes.
            tx_inputs.update_notes(candidate_notes.clone().into());
            match self.try_execute_notes(&tx_inputs, &tx_args).await {
                Ok(()) => {
                    // A full set of successful notes has been found.
                    let successful = candidate_notes;
                    return Ok(NoteConsumptionInfo::new(successful, failed_notes));
                },
                Err(TransactionCheckerError::NoteExecution { failed_note_index, error }) => {
                    // SAFETY: Failed note index is in bounds of the candidate notes.
                    let failed_note = candidate_notes.remove(failed_note_index);
                    failed_notes.push(FailedNote::new(failed_note, error));

                    // All possible candidate combinations have been attempted.
                    if candidate_notes.is_empty() {
                        return Ok(NoteConsumptionInfo::new(Vec::new(), failed_notes));
                    }
                    // Continue and process the next set of candidates.
                },
                Err(TransactionCheckerError::EpilogueExecution(_)) => {
                    let consumption_info = self
                        .find_largest_executable_combination(
                            candidate_notes,
                            failed_notes,
                            tx_inputs,
                            &tx_args,
                        )
                        .await;
                    return Ok(consumption_info);
                },
                Err(TransactionCheckerError::PrologueExecution(err)) => {
                    return Err(NoteCheckerError::PrologueExecution(err));
                },
                Err(TransactionCheckerError::TransactionPreparation(err)) => {
                    return Err(NoteCheckerError::TransactionPreparation(err));
                },
            }
        }
    }

    /// Attempts to find the largest possible combination of notes that can execute successfully
    /// together.
    ///
    /// This method incrementally tries combinations of increasing size (1 note, 2 notes, 3 notes,
    /// etc.) and builds upon previously successful combinations to find the maximum executable
    /// set.
    async fn find_largest_executable_combination(
        &self,
        mut remaining_notes: Vec<Note>,
        mut failed_notes: Vec<FailedNote>,
        mut tx_inputs: TransactionInputs,
        tx_args: &TransactionArgs,
    ) -> NoteConsumptionInfo {
        let mut successful_notes = Vec::new();
        let mut failed_note_index = BTreeMap::new();

        // Iterate by note count: try 1 note, then 2, then 3, etc.
        for size in 1..=remaining_notes.len() {
            // Can't build a combination of size N without at least N-1 successful notes.
            if successful_notes.len() < size - 1 {
                break;
            }

            // Try adding each remaining note to the current successful combination.
            for (idx, note) in remaining_notes.iter().enumerate() {
                successful_notes.push(note.clone());

                tx_inputs.update_notes(successful_notes.clone().into());
                match self.try_execute_notes(&tx_inputs, tx_args).await {
                    Ok(()) => {
                        // The successfully added note might have failed earlier. Remove it from the
                        // failed list.
                        failed_note_index.remove(&note.id());
                        // This combination succeeded; remove the most recently added note from
                        // the remaining set.
                        remaining_notes.remove(idx);
                        break;
                    },
                    Err(error) => {
                        // This combination failed; remove the last note from the test set and
                        // continue to next note.
                        let failed_note =
                            successful_notes.pop().expect("successful notes should not be empty");
                        // Record the failed note (overwrite previous failures for the relevant
                        // note).
                        failed_note_index
                            .insert(failed_note.id(), FailedNote::new(failed_note, error.into()));
                    },
                }
            }
        }

        // Append failed notes to the list of failed notes provided as input.
        failed_notes.extend(failed_note_index.into_values());
        NoteConsumptionInfo::new(successful_notes, failed_notes)
    }

    /// Attempts to execute a transaction with the provided input notes.
    ///
    /// This method executes the full transaction pipeline including prologue, note execution,
    /// and epilogue phases. It returns `Ok(())` if all notes are successfully consumed,
    /// or a specific [`NoteExecutionError`] indicating where and why the execution failed.
    async fn try_execute_notes(
        &self,
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
    ) -> Result<(), TransactionCheckerError> {
        if tx_inputs.input_notes().is_empty() {
            return Ok(());
        }

        let (mut host, stack_inputs, advice_inputs) = self
            .0
            .prepare_transaction(tx_inputs, tx_args, None)
            .await
            .map_err(TransactionCheckerError::TransactionPreparation)?;

        let processor =
            FastProcessor::new_with_advice_inputs(stack_inputs.as_slice(), advice_inputs);
        let result = processor
            .execute(&TransactionKernel::main(), &mut host)
            .await
            .map_err(TransactionExecutorError::TransactionProgramExecutionFailed);

        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                let notes = host.tx_progress().note_execution();

                // Empty notes vector means that we didn't process the notes, so an error
                // occurred.
                if notes.is_empty() {
                    return Err(TransactionCheckerError::PrologueExecution(error));
                }

                let ((_, last_note_interval), success_notes) =
                    notes.split_last().expect("notes vector is not empty because of earlier check");

                // If the interval end of the last note is specified, then an error occurred after
                // notes processing.
                if last_note_interval.end().is_some() {
                    Err(TransactionCheckerError::EpilogueExecution(error))
                } else {
                    // Return the index of the failed note.
                    let failed_note_index = success_notes.len();
                    Err(TransactionCheckerError::NoteExecution { failed_note_index, error })
                }
            },
        }
    }
}
