use alloc::vec::Vec;

use miden_protocol::errors::NoteError;
use miden_protocol::note::{NoteAttachment, NoteRecipient, NoteStorage};
use miden_protocol::{Felt, MAX_NOTE_STORAGE_ITEMS, Word};

/// Represents the different storage formats for MINT notes.
/// - Private: Creates a private output note using a precomputed recipient digest (12 MINT note
///   storage items)
/// - Public: Creates a public output note by providing script root, serial number, and
///   variable-length storage (16+ MINT note storage items: 16 fixed + variable number of output
///   note storage items)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MintNoteStorage {
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

impl MintNoteStorage {
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
        // Calculate total number of storage items that will be created:
        // 16 fixed items (tag, amount, attachment_kind, attachment_scheme, ATTACHMENT,
        // SCRIPT_ROOT, SERIAL_NUM) + variable recipient storage length
        const FIXED_PUBLIC_STORAGE_ITEMS: usize = 16;
        let total_storage_items = FIXED_PUBLIC_STORAGE_ITEMS + recipient.storage().len() as usize;

        if total_storage_items > MAX_NOTE_STORAGE_ITEMS {
            return Err(NoteError::TooManyStorageItems(total_storage_items));
        }

        Ok(Self::Public {
            recipient,
            amount,
            tag,
            attachment: NoteAttachment::default(),
        })
    }

    /// Overwrites the [`NoteAttachment`] of the note storage.
    pub fn with_attachment(self, attachment: NoteAttachment) -> Self {
        match self {
            MintNoteStorage::Private {
                recipient_digest,
                amount,
                tag,
                attachment: _,
            } => MintNoteStorage::Private {
                recipient_digest,
                amount,
                tag,
                attachment,
            },
            MintNoteStorage::Public { recipient, amount, tag, attachment: _ } => {
                MintNoteStorage::Public { recipient, amount, tag, attachment }
            },
        }
    }
}

impl From<MintNoteStorage> for NoteStorage {
    fn from(mint_storage: MintNoteStorage) -> Self {
        match mint_storage {
            MintNoteStorage::Private {
                recipient_digest,
                amount,
                tag,
                attachment,
            } => {
                let attachment_scheme = Felt::from(attachment.attachment_scheme().as_u32());
                let attachment_kind = Felt::from(attachment.attachment_kind().as_u8());
                let attachment = attachment.content().to_word();

                let mut storage_values = Vec::with_capacity(12);
                storage_values.extend_from_slice(&[
                    tag,
                    amount,
                    attachment_kind,
                    attachment_scheme,
                ]);
                storage_values.extend_from_slice(attachment.as_elements());
                storage_values.extend_from_slice(recipient_digest.as_elements());
                NoteStorage::new(storage_values)
                    .expect("number of storage items should not exceed max storage items")
            },
            MintNoteStorage::Public { recipient, amount, tag, attachment } => {
                let attachment_scheme = Felt::from(attachment.attachment_scheme().as_u32());
                let attachment_kind = Felt::from(attachment.attachment_kind().as_u8());
                let attachment = attachment.content().to_word();

                let mut storage_values = vec![tag, amount, attachment_kind, attachment_scheme];
                storage_values.extend_from_slice(attachment.as_elements());
                storage_values.extend_from_slice(recipient.script().root().as_elements());
                storage_values.extend_from_slice(recipient.serial_num().as_elements());
                storage_values.extend_from_slice(recipient.storage().items());
                NoteStorage::new(storage_values)
                    .expect("number of storage items should not exceed max storage items")
            },
        }
    }
}
