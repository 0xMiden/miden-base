use alloc::vec::Vec;

use miden_protocol::note::{NoteAttachment, NoteInputs, NoteRecipient};
use miden_protocol::{Felt, MAX_INPUTS_PER_NOTE, NoteError, Word};

/// Represents the different input formats for MINT notes.
/// - Private: Creates a private output note using a precomputed recipient digest (12 MINT note
///   inputs)
/// - Public: Creates a public output note by providing script root, serial number, and
///   variable-length inputs (16+ MINT note inputs: 16 fixed + variable number of output note
///   inputs)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MintNoteInputs {
    Private {
        recipient_digest: Word,
        amount: Felt,
        tag: Felt,
        attachment: NoteAttachment,
    },
    Public {
        recipient: NoteRecipient,
        amount: Felt,
        tag: Felt,
        attachment: NoteAttachment,
    },
}

impl MintNoteInputs {
    pub fn new_private(recipient_digest: Word, amount: Felt, tag: Felt) -> Self {
        Self::Private {
            recipient_digest,
            amount,
            tag,
            attachment: NoteAttachment::default(),
        }
    }

    pub fn new_public(
        recipient: NoteRecipient,
        amount: Felt,
        tag: Felt,
    ) -> Result<Self, NoteError> {
        // Calculate total number of inputs that will be created:
        // 16 fixed inputs (tag, amount, attachment_type, attachment_content_type, ATTACHMENT,
        // SCRIPT_ROOT, SERIAL_NUM) + variable recipient inputs length
        const FIXED_PUBLIC_INPUTS: usize = 16;
        let total_inputs = FIXED_PUBLIC_INPUTS + recipient.inputs().num_values() as usize;

        if total_inputs > MAX_INPUTS_PER_NOTE {
            return Err(NoteError::TooManyInputs(total_inputs));
        }

        Ok(Self::Public {
            recipient,
            amount,
            tag,
            attachment: NoteAttachment::default(),
        })
    }

    /// Overwrites the [`NoteAttachment`] of the note inputs.
    pub fn with_attachment(self, attachment: NoteAttachment) -> Self {
        match self {
            MintNoteInputs::Private {
                recipient_digest,
                amount,
                tag,
                attachment: _,
            } => MintNoteInputs::Private {
                recipient_digest,
                amount,
                tag,
                attachment,
            },
            MintNoteInputs::Public { recipient, amount, tag, attachment: _ } => {
                MintNoteInputs::Public { recipient, amount, tag, attachment }
            },
        }
    }
}

impl From<MintNoteInputs> for NoteInputs {
    fn from(mint_inputs: MintNoteInputs) -> Self {
        match mint_inputs {
            MintNoteInputs::Private {
                recipient_digest,
                amount,
                tag,
                attachment,
            } => {
                let attachment_type = Felt::from(attachment.attachment_type().as_u32());
                let attachment_content_type = Felt::from(attachment.content_type().as_u8());
                let attachment = attachment.content().to_word();

                let mut input_values = Vec::with_capacity(12);
                input_values.extend_from_slice(&[
                    tag,
                    amount,
                    attachment_type,
                    attachment_content_type,
                ]);
                input_values.extend_from_slice(attachment.as_elements());
                input_values.extend_from_slice(recipient_digest.as_elements());
                NoteInputs::new(input_values)
                    .expect("number of inputs should not exceed max inputs")
            },
            MintNoteInputs::Public { recipient, amount, tag, attachment } => {
                let attachment_type = Felt::from(attachment.attachment_type().as_u32());
                let attachment_content_type = Felt::from(attachment.content_type().as_u8());
                let attachment = attachment.content().to_word();

                let mut input_values = vec![tag, amount, attachment_type, attachment_content_type];
                input_values.extend_from_slice(attachment.as_elements());
                input_values.extend_from_slice(recipient.script().root().as_elements());
                input_values.extend_from_slice(recipient.serial_num().as_elements());
                input_values.extend_from_slice(recipient.inputs().values());
                NoteInputs::new(input_values)
                    .expect("number of inputs should not exceed max inputs")
            },
        }
    }
}
