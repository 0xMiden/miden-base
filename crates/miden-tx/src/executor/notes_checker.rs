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
use crate::errors::TransactionCheckerError;
use crate::{DataStore, NoteCheckerError, TransactionExecutorError};

// CONSTANTS
// ================================================================================================

/// Maximum number of notes that can be checked at once.
pub const MAX_NUM_CHECKER_NOTES: u16 = 20;

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
    /// [`Self::INPUT_NOTES_RANGE`] input notes is allowed.
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
        input_notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
    ) -> Result<NoteConsumptionInfo, NoteCheckerError> {
        let num_notes = input_notes.num_notes();
        if num_notes == 0 || num_notes > MAX_NUM_CHECKER_NOTES {
            return Err(NoteCheckerError::InputNoteCountOutOfRange(num_notes));
        }
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
    ) -> Result<NoteConsumptionInfo, NoteCheckerError> {
        let mut candidate_notes = notes.into_vec();
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
                .await
            {
                Ok(()) => {
                    // A full set of successful notes has been found.
                    let successful =
                        candidate_notes.into_iter().map(InputNote::into_note).collect::<Vec<_>>();
                    return Ok(NoteConsumptionInfo::new(successful, failed_notes));
                },
                Err(TransactionCheckerError::NoteExecution { failed_note_index, error }) => {
                    // SAFETY: Failed note index is in bounds of the candidate notes.
                    let failed_note = candidate_notes.remove(failed_note_index).into_note();
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
                            target_account_id,
                            block_ref,
                            candidate_notes,
                            failed_notes,
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
        target_account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: Vec<InputNote>,
        mut failed_notes: Vec<FailedNote>,
        tx_args: &TransactionArgs,
    ) -> NoteConsumptionInfo {
        let mut successful_notes = Vec::new();
        let mut remaining_notes = input_notes;

        // Iterate by note count: try 1 note, then 2, then 3, etc.
        for size in 1..=remaining_notes.len() {
            // Can't build a combination of size N without at least N-1 successful notes.
            if successful_notes.len() < size - 1 {
                break;
            }

            // Try adding each remaining note to the current successful combination.
            for (idx, note) in remaining_notes.iter().enumerate() {
                successful_notes.push(note.clone());

                match self
                    .try_execute_notes(
                        target_account_id,
                        block_ref,
                        InputNotes::<InputNote>::new_unchecked(successful_notes.clone()),
                        tx_args,
                    )
                    .await
                {
                    Ok(()) => {
                        // This combination succeeded; remove the most recently added note from
                        // the remaining set.
                        remaining_notes.remove(idx);
                        break;
                    },
                    Err(error) => {
                        // This combination failed; remove the last note from the test set and
                        // continue to next note.
                        let failed_note =
                            successful_notes.pop().expect("Successful notes should not be empty");
                        // Record the failed note.
                        failed_notes.push(FailedNote::new(failed_note.into_note(), error.into()));
                    },
                }
            }
        }

        // Convert successful InputNotes to Notes.
        let successful = successful_notes.into_iter().map(InputNote::into_note).collect::<Vec<_>>();
        NoteConsumptionInfo::new(successful, failed_notes)
    }

    /// Attempts to execute a transaction with the provided input notes.
    ///
    /// This method executes the full transaction pipeline including prologue, note execution,
    /// and epilogue phases. It returns `Ok(())` if all notes are successfully consumed,
    /// or a specific [`NoteExecutionError`] indicating where and why the execution failed.
    async fn try_execute_notes(
        &self,
        account_id: AccountId,
        block_ref: BlockNumber,
        notes: InputNotes<InputNote>,
        tx_args: &TransactionArgs,
    ) -> Result<(), TransactionCheckerError> {
        if notes.is_empty() {
            return Ok(());
        }

        // TODO: ideally, we should prepare the inputs only once for the whole note consumption
        // check (rather than doing this every time when we try to execute some subset of notes),
        // but we currently cannot do this because transaction preparation includes input notes;
        // we should refactor the preparation process to separate input note preparation from the
        // rest, and then we can prepare the rest of the inputs once for the whole check
        let (mut host, _, stack_inputs, advice_inputs) = self
            .0
            .prepare_transaction(account_id, block_ref, notes, tx_args, None)
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
