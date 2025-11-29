use alloc::vec::Vec;

use miden_objects::note::{NoteExecutionHint, NoteInputs};
use miden_objects::{Felt, NoteError, Word};

/// Represents the different input formats for MINT notes.
/// - Private: Creates a private output note using a precomputed recipient digest (8 inputs)
/// - Public: Creates a public output note by providing script root, serial number, and inputs (16
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
        inputs: Word,
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
        if input_values.len() > 4 {
            return Err(NoteError::other(
                "public output note inputs cannot have more than 4 elements",
            ));
        }

        let mut padded_inputs = input_values;
        padded_inputs.resize(4, Felt::new(0));

        let inputs =
            Word::from([padded_inputs[3], padded_inputs[2], padded_inputs[1], padded_inputs[0]]);

        // Reverse word order to match the expected order in the MINT script
        let script_root_be =
            Word::from([script_root[3], script_root[2], script_root[1], script_root[0]]);
        let serial_num_be =
            Word::from([serial_num[3], serial_num[2], serial_num[1], serial_num[0]]);

        Ok(Self::Public {
            script_root: script_root_be,
            serial_num: serial_num_be,
            inputs,
            amount,
            tag,
            execution_hint,
            aux,
        })
    }

    pub fn amount(&self) -> Felt {
        match self {
            Self::Private { amount, .. } => *amount,
            Self::Public { amount, .. } => *amount,
        }
    }

    pub fn tag(&self) -> Felt {
        match self {
            Self::Private { tag, .. } => *tag,
            Self::Public { tag, .. } => *tag,
        }
    }

    pub fn execution_hint(&self) -> NoteExecutionHint {
        match self {
            Self::Private { execution_hint, .. } => *execution_hint,
            Self::Public { execution_hint, .. } => *execution_hint,
        }
    }

    pub fn aux(&self) -> Felt {
        match self {
            Self::Private { aux, .. } => *aux,
            Self::Public { aux, .. } => *aux,
        }
    }

    pub fn is_private(&self) -> bool {
        matches!(self, Self::Private { .. })
    }

    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public { .. })
    }

    pub fn num_inputs(&self) -> usize {
        match self {
            Self::Private { .. } => 8,
            Self::Public { .. } => 16,
        }
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
                let inputs = vec![
                    execution_hint.into(),
                    aux,
                    tag,
                    amount,
                    recipient_digest[0],
                    recipient_digest[1],
                    recipient_digest[2],
                    recipient_digest[3],
                ];
                NoteInputs::new(inputs)
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
                let input_values = vec![
                    execution_hint.into(),
                    aux,
                    tag,
                    amount,
                    script_root[3],
                    script_root[2],
                    script_root[1],
                    script_root[0],
                    serial_num[3],
                    serial_num[2],
                    serial_num[1],
                    serial_num[0],
                    inputs[3],
                    inputs[2],
                    inputs[1],
                    inputs[0],
                ];
                NoteInputs::new(input_values)
            },
        }
    }
}
