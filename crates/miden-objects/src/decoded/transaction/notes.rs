pub use proto::transaction::DecodedInputNoteCommitment as InputNoteCommitment;

use crate::{BuildUnchecked, Verify, proto};

#[cfg(test)]
mod tests;

/// Builds the decoded commitment without checking that its nullifier belongs to its note header.
/// A present header is verified, but the caller must establish nullifier/header consistency and
/// authenticate the note's inclusion separately. An absent header is not evidence of inclusion.
impl BuildUnchecked for InputNoteCommitment {
    type Output = miden_protocol::transaction::InputNoteCommitment;
    type Error = crate::decoded::note::VerificationError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        Ok(Self::Output::from_parts_unchecked(
            miden_protocol::note::Nullifier::from_raw(self.nullifier),
            self.header.map(Verify::verify).transpose()?,
        ))
    }
}

pub use proto::transaction::DecodedPrivateOutputNote as PrivateOutputNote;

impl Verify for PrivateOutputNote {
    type Verified = miden_protocol::transaction::PrivateOutputNote;
    type Error = PrivateOutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.header.verify()?, self.attachments.verify()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PrivateOutputNoteError {
    #[error("invalid note: {0}")]
    Note(#[from] crate::decoded::note::VerificationError),
    #[error("invalid private output note: {0}")]
    Output(#[from] miden_protocol::errors::OutputNoteError),
}

pub use proto::transaction::DecodedPublicOutputNote as PublicOutputNote;

impl Verify for PublicOutputNote {
    type Verified = miden_protocol::transaction::PublicOutputNote;
    type Error = PublicOutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.note.verify()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublicOutputNoteError {
    #[error("{0}")]
    Note(#[from] crate::decoded::note::VerificationError),
    #[error("{0}")]
    Output(#[from] miden_protocol::errors::OutputNoteError),
}

pub use proto::transaction::DecodedOutputNote as OutputNote;

impl Verify for OutputNote {
    type Verified = miden_protocol::transaction::OutputNote;
    type Error = OutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::transaction::output_note::DecodedNote;
        match self.note {
            DecodedNote::Public(note) => Ok(Self::Verified::Public(note.verify()?)),
            DecodedNote::Private(note) => Ok(Self::Verified::Private(note.verify()?)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OutputNoteError {
    #[error("{0}")]
    Public(#[from] PublicOutputNoteError),
    #[error("{0}")]
    Private(#[from] PrivateOutputNoteError),
}

pub use proto::transaction::DecodedAuthenticatedInputNote as AuthenticatedInputNote;

/// Checks proof/note identity, not inclusion against a trusted block root.
impl Verify for AuthenticatedInputNote {
    type Verified = miden_protocol::transaction::InputNote;
    type Error = InputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let note = self.note.verify()?;
        let (proof_id, proof) = self.proof.verify()?;
        if proof_id != note.id() {
            return Err(InputNoteError::IdMismatch {
                transmitted: proof_id,
                decoded: note.id(),
            });
        }
        Ok(Self::Verified::authenticated(note, proof))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputNoteError {
    #[error("{0}")]
    Note(#[from] crate::decoded::note::VerificationError),
    #[error("note ID mismatch: transmitted {transmitted}, decoded {decoded}")]
    IdMismatch {
        transmitted: miden_protocol::note::NoteId,
        decoded: miden_protocol::note::NoteId,
    },
}

pub use proto::transaction::DecodedInputNote as InputNote;

impl Verify for InputNote {
    type Verified = miden_protocol::transaction::InputNote;
    type Error = InputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::transaction::input_note::DecodedNote;
        match self.note {
            DecodedNote::Authenticated(note) => note.verify(),
            DecodedNote::Unauthenticated(note) => {
                Ok(Self::Verified::unauthenticated(note.verify()?))
            },
        }
    }
}

pub use proto::transaction::DecodedInputNotes as InputNotes;

impl Verify for InputNotes {
    type Verified = miden_protocol::transaction::InputNotes<miden_protocol::transaction::InputNote>;
    type Error = InputNotesError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let notes = self.notes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        Ok(Self::Verified::new(notes)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputNotesError {
    #[error("{0}")]
    Note(#[from] InputNoteError),
    #[error("{0}")]
    Input(#[from] miden_protocol::errors::TransactionInputError),
}
