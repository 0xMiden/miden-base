use miden_protocol::note::NoteAttachmentType;

/// The [`NoteAttachmentType`]s of well-known note attachmens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WellKnownNoteAttachment {
    /// See [`NetworkAccountTarget`](crate::note::NetworkAccountTarget) for details.
    NetworkAccountTarget,
}

impl WellKnownNoteAttachment {
    /// Returns the [`NoteAttachmentType`] of the well-known attachment.
    pub const fn attachment_type(&self) -> NoteAttachmentType {
        match self {
            WellKnownNoteAttachment::NetworkAccountTarget => NoteAttachmentType::new(1u32),
        }
    }
}
