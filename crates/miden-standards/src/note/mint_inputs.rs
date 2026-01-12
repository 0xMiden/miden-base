use alloc::vec::Vec;

use miden_protocol::note::{NoteInputs, NoteRecipient};
use miden_protocol::{Felt, FieldElement, MAX_INPUTS_PER_NOTE, NoteError, Word};

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
    },
    Public {
        recipient: NoteRecipient,
        amount: Felt,
        tag: Felt,
    },
}

impl MintNoteInputs {
    pub fn new_private(recipient_digest: Word, amount: Felt, tag: Felt) -> Self {
        Self::Private { recipient_digest, amount, tag }
    }

    pub fn new_public(
        recipient: NoteRecipient,
        amount: Felt,
        tag: Felt,
    ) -> Result<Self, NoteError> {
        // Calculate total number of inputs that will be created:
        // 12 fixed inputs (execution_hint, aux, tag, amount, SCRIPT_ROOT, SERIAL_NUM) +
        // variable recipient inputs length
        const FIXED_PUBLIC_INPUTS: usize = 12;
        let total_inputs = FIXED_PUBLIC_INPUTS + recipient.inputs().num_values() as usize;

        if total_inputs > MAX_INPUTS_PER_NOTE {
            return Err(NoteError::TooManyInputs(total_inputs));
        }

        Ok(Self::Public { recipient, amount, tag })
    }
}

impl From<MintNoteInputs> for NoteInputs {
    fn from(mint_inputs: MintNoteInputs) -> Self {
        match mint_inputs {
            MintNoteInputs::Private { recipient_digest, amount, tag } => {
                let mut input_values = Vec::with_capacity(6);
                input_values.extend_from_slice(recipient_digest.as_elements());
                input_values.extend_from_slice(&[tag, amount]);
                NoteInputs::new(input_values)
                    .expect("number of inputs should not exceed max inputs")
            },
            MintNoteInputs::Public { recipient, amount, tag } => {
                // Pad with zeroes to make the inputs pointer word-aligned.
                let mut input_values = vec![tag, amount, Felt::ZERO, Felt::ZERO];
                input_values.extend_from_slice(recipient.script().root().as_elements());
                input_values.extend_from_slice(recipient.serial_num().as_elements());
                input_values.extend_from_slice(recipient.inputs().values());
                NoteInputs::new(input_values)
                    .expect("number of inputs should not exceed max inputs")
            },
        }
    }
}
