use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::account::interface::NoteAccountCompatibility;
use miden_lib::note::well_known_note::WellKnownNote;
use miden_objects::account::AccountId;
use miden_objects::assembly::SourceManager;
use miden_objects::block::BlockNumber;
use miden_objects::note::Note;
use miden_objects::transaction::{InputNote, InputNotes, TransactionArgs};
use winter_maybe_async::{maybe_async, maybe_await};

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

    /// Checks whether the provided input notes could be consumed by the provided account.
    ///
    /// This check consists of two main steps:
    /// - Statically check the notes: if all notes are either `P2ID` or `P2IDE` notes with correct
    ///   inputs.
    /// - Execute the transaction.
    #[maybe_async]
    pub fn check_notes_consumability(
        &self,
        target_account_id: AccountId,
        block_ref: BlockNumber,
        input_notes: InputNotes<InputNote>,
        tx_args: TransactionArgs,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<NoteConsumptionInfo, TransactionExecutorError> {
        let input_note_count = input_notes.num_notes() as usize;
        let mut successful = vec![];
        let mut failed = vec![];
        let mut maybe = vec![];
        for note in input_notes.into_iter() {
            if let Some(well_known_note) = WellKnownNote::from_note(note.note()) {
                if let WellKnownNote::SWAP = well_known_note {
                    // If we encountered a SWAP note, then we have to execute the transaction
                    // anyway, but we should continue iterating to make sure that there are no
                    // P2IDE notes.
                    maybe.push(note);
                    continue;
                }

                match well_known_note.check_note_inputs(note.note(), target_account_id, block_ref) {
                    NoteAccountCompatibility::No => {
                        // If the check failed register the note as failed.
                        failed.push(NoteConsumptionError::AccountCompatibilityError(
                            note.into_note(),
                        ));
                    },

                    // This branch is unreachable, since we are handling the SWAP note separately,
                    // but as an extra precaution continue iterating over the notes and run the
                    // transaction to make sure the note which returned "Maybe" could be consumed.
                    NoteAccountCompatibility::Maybe => {
                        maybe.push(note);
                    },
                    NoteAccountCompatibility::Yes => {
                        // Put the successfully checked `P2ID` or `P2IDE` note to the vector.
                        successful.push(note.into_note());
                    },
                }
            } else {
                // If we encountered not a well known note, then we have to execute the transaction
                // anyway, but we should continue iterating to make sure that there are no
                // P2IDE notes which return a `No`.
                maybe.push(note);
                continue;
            }
        }

        // If all checked notes turned out to be either `P2ID` or `P2IDE` notes and all of them
        // passed, then we could safely return the `Success`.
        if successful.len() == input_note_count {
            return Ok(NoteConsumptionInfo::new_successful(successful));
        }

        // Execute transaction.
        let mut consumption_info = maybe_await!(self.0.try_execute_notes(
            target_account_id,
            block_ref,
            // NOTE: these notes were moved from a well-formed `InputNotes<InputNote>`.
            InputNotes::new_unchecked(maybe),
            tx_args,
            source_manager
        ))?;

        // Combine all successful and failed notes into the consumption info.
        consumption_info.failed.extend(failed);
        consumption_info.successful.extend(successful);
        Ok(consumption_info)
    }
}
