use alloc::vec::Vec;

use miden_objects::note::{NoteExecutionHint, NoteInputs};
use miden_objects::{Felt, NoteError, Word};

/// Represents the different input formats for MINT notes.
/// - Private: Creates a private output note using a precomputed recipient digest (8 MINT note
///   inputs)
/// - Public: Creates a public output note by providing script root, serial number, and
///   variable-length inputs (12+ MINT note inputs: 12 fixed + variable number of output note
///   inputs)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MintNoteInputs {
    Private {
        recipient_digest: Word,
        amount: Felt,
        tag: Felt,
        execution_hint: NoteExecutionHint,
        aux: Felt,
    },
    Public {
        script_root: Word,
        serial_num: Word,
        inputs: Vec<Felt>,
        amount: Felt,
        tag: Felt,
        execution_hint: NoteExecutionHint,
        aux: Felt,
    },
}

impl MintNoteInputs {
    pub fn new_private(
        recipient_digest: Word,
        amount: Felt,
        tag: Felt,
        execution_hint: NoteExecutionHint,
        aux: Felt,
    ) -> Self {
        Self::Private {
            recipient_digest,
            amount,
            tag,
            execution_hint,
            aux,
        }
    }

    pub fn new_public(
        script_root: Word,
        serial_num: Word,
        input_values: Vec<Felt>,
        amount: Felt,
        tag: Felt,
        execution_hint: NoteExecutionHint,
        aux: Felt,
    ) -> Result<Self, NoteError> {
        // No limit on input_values length since inputs are at the end of MINT note inputs
        // The MINT note will compute NOTE_INPUTS - 12 to determine the number of output note inputs

        Ok(Self::Public {
            script_root,
            serial_num,
            inputs: input_values,
            amount,
            tag,
            execution_hint,
            aux,
        })
    }
}

impl TryFrom<MintNoteInputs> for NoteInputs {
    type Error = NoteError;

    fn try_from(mint_inputs: MintNoteInputs) -> Result<Self, Self::Error> {
        match mint_inputs {
            MintNoteInputs::Private {
                recipient_digest,
                amount,
                tag,
                execution_hint,
                aux,
            } => {
                let mut input_values = vec![execution_hint.into(), aux, tag, amount];
                input_values.extend_from_slice(recipient_digest.as_elements());
                NoteInputs::new(input_values)
            },
            MintNoteInputs::Public {
                script_root,
                serial_num,
                inputs,
                amount,
                tag,
                execution_hint,
                aux,
            } => {
                let mut input_values = vec![execution_hint.into(), aux, tag, amount];
                input_values.extend_from_slice(script_root.as_elements());
                input_values.extend_from_slice(serial_num.as_elements());
                input_values.extend(inputs);
                NoteInputs::new(input_values)
            },
        }
    }
}
