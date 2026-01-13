use miden_protocol::note::NoteAttachmentScheme;

/// The [`NoteAttachmentScheme`]s of well-known note attachmens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WellKnownNoteAttachment {
    /// See [`NetworkAccountTarget`](crate::note::NetworkAccountTarget) for details.
    NetworkAccountTarget,
}

impl WellKnownNoteAttachment {
    /// Returns the [`NoteAttachmentScheme`] of the well-known attachment.
    pub const fn attachment_scheme(&self) -> NoteAttachmentScheme {
        match self {
            WellKnownNoteAttachment::NetworkAccountTarget => NoteAttachmentScheme::new(1u32),
        }
    }
}
