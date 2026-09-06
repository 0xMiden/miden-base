use alloc::format;
use alloc::vec::Vec;

use miden_protocol::account::{AccountId, AccountUpdateDetails};
use miden_protocol::transaction::{
    InputNoteCommitment,
    OutputNote,
    PrivateOutputNote,
    ProvenTransaction,
    PublicOutputNote,
    TransactionArgs,
    TransactionHeader,
    TransactionId,
    TransactionScript,
    TxAccountUpdate,
};

use super::{MessageDecodeExt, required};
use crate::{ConversionError, ConversionResultExt, proto};

// TRANSACTION ARGUMENTS
// ================================================================================================

impl From<&TransactionScript> for proto::transaction::TransactionScript {
    fn from(value: &TransactionScript) -> Self {
        Self {
            entrypoint: value.entrypoint().into(),
            mast: Some(value.mast().as_ref().into()),
        }
    }
}

impl From<&TransactionArgs> for proto::transaction::TransactionArgs {
    fn from(value: &TransactionArgs) -> Self {
        Self {
            tx_script: value.tx_script().map(Into::into),
            tx_script_args: Some(value.tx_script_args().into()),
            note_args: value
                .note_args()
                .iter()
                .map(|(note_id, args)| proto::transaction::NoteArgument {
                    note_id: Some(note_id.into()),
                    args: Some(args.into()),
                })
                .collect(),
            advice_inputs: Some(value.advice_inputs().into()),
            auth_args: Some(value.auth_args().into()),
        }
    }
}

impl From<TransactionArgs> for proto::transaction::TransactionArgs {
    fn from(value: TransactionArgs) -> Self {
        (&value).into()
    }
}

// TX ACCOUNT UPDATE
// ================================================================================================

impl From<&TxAccountUpdate> for proto::transaction::TxAccountUpdate {
    fn from(value: &TxAccountUpdate) -> Self {
        Self {
            account_id: Some(value.account_id().into()),
            initial_state_commitment: Some(value.initial_state_commitment().into()),
            final_state_commitment: Some(value.final_state_commitment().into()),
            account_patch_commitment: Some(value.account_patch_commitment().into()),
            details: Some(value.details().into()),
        }
    }
}

impl TryFrom<proto::transaction::TxAccountUpdate> for TxAccountUpdate {
    type Error = ConversionError;

    fn try_from(value: proto::transaction::TxAccountUpdate) -> Result<Self, Self::Error> {
        let decoder = value.decoder();
        let account_id: AccountId = required!(decoder, value.account_id)?;
        let initial_state_commitment = required!(decoder, value.initial_state_commitment)?;
        let final_state_commitment = required!(decoder, value.final_state_commitment)?;
        let account_patch_commitment = required!(decoder, value.account_patch_commitment)?;
        let details: AccountUpdateDetails = required!(decoder, value.details)?;
        Self::new(
            account_id,
            initial_state_commitment,
            final_state_commitment,
            account_patch_commitment,
            details,
        )
        .map_err(ConversionError::new)
    }
}

// PROVEN TRANSACTION
// ================================================================================================

impl From<&ProvenTransaction> for proto::transaction::ProvenTransaction {
    fn from(value: &ProvenTransaction) -> Self {
        Self {
            account_update: Some(value.account_update().into()),
            input_notes: value.input_notes().iter().map(Into::into).collect(),
            output_notes: value.output_notes().iter().map(Into::into).collect(),
            reference_block_num: Some(value.ref_block_num().into()),
            reference_block_commitment: Some(value.ref_block_commitment().into()),
            expiration_block_num: Some(value.expiration_block_num().into()),
            proof: Some(value.proof().into()),
        }
    }
}

impl From<ProvenTransaction> for proto::transaction::ProvenTransaction {
    fn from(value: ProvenTransaction) -> Self {
        Self::from(&value)
    }
}

impl TryFrom<proto::transaction::ProvenTransaction> for ProvenTransaction {
    type Error = ConversionError;

    fn try_from(value: proto::transaction::ProvenTransaction) -> Result<Self, Self::Error> {
        let decoder = value.decoder();
        let account_update = required!(decoder, value.account_update)?;
        let input_notes = value
            .input_notes
            .into_iter()
            .enumerate()
            .map(|(index, note)| {
                InputNoteCommitment::try_from(note).context(format!("input_notes[{index}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let output_notes = value
            .output_notes
            .into_iter()
            .enumerate()
            .map(|(index, note)| {
                OutputNote::try_from(note).context(format!("output_notes[{index}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reference_block_commitment = required!(decoder, value.reference_block_commitment)?;
        let reference_block_num =
            required!(decoder, value.reference_block_num).context("reference_block_num")?;
        let expiration_block_num =
            required!(decoder, value.expiration_block_num).context("expiration_block_num")?;
        let proof = required!(decoder, value.proof)?;

        Self::new(
            account_update,
            input_notes,
            output_notes,
            reference_block_num,
            reference_block_commitment,
            expiration_block_num,
            proof,
        )
        .map_err(ConversionError::new)
    }
}

// FROM TRANSACTION ID
// ================================================================================================

impl From<&TransactionId> for proto::transaction::TransactionId {
    fn from(value: &TransactionId) -> Self {
        proto::transaction::TransactionId { id: Some(value.as_word().into()) }
    }
}

impl From<TransactionId> for proto::transaction::TransactionId {
    fn from(value: TransactionId) -> Self {
        (&value).into()
    }
}

// INTO TRANSACTION ID
// ================================================================================================

// INPUT NOTE COMMITMENT
// ================================================================================================

impl From<InputNoteCommitment> for proto::transaction::InputNoteCommitment {
    fn from(value: InputNoteCommitment) -> Self {
        Self::from(&value)
    }
}

impl From<&InputNoteCommitment> for proto::transaction::InputNoteCommitment {
    fn from(value: &InputNoteCommitment) -> Self {
        Self {
            nullifier: Some(value.nullifier().as_word().into()),
            header: value.header().copied().map(Into::into),
        }
    }
}

// TRANSACTION HEADER
// ================================================================================================

impl From<&TransactionHeader> for proto::transaction::TransactionHeader {
    fn from(header: &TransactionHeader) -> Self {
        Self {
            transaction_id: Some(header.id().into()),
            account_id: Some(header.account_id().into()),
            initial_state_commitment: Some(header.initial_state_commitment().into()),
            final_state_commitment: Some(header.final_state_commitment().into()),
            input_notes: header.input_notes().iter().map(Into::into).collect(),
            output_notes: header.output_notes().iter().copied().map(Into::into).collect(),
        }
    }
}

impl From<TransactionHeader> for proto::transaction::TransactionHeader {
    fn from(header: TransactionHeader) -> Self {
        Self::from(&header)
    }
}

// OUTPUT NOTES
// ================================================================================================

impl From<&PublicOutputNote> for proto::transaction::PublicOutputNote {
    fn from(note: &PublicOutputNote) -> Self {
        Self {
            note: Some(note.as_note().clone().into()),
        }
    }
}

impl From<PublicOutputNote> for proto::transaction::PublicOutputNote {
    fn from(note: PublicOutputNote) -> Self {
        Self::from(&note)
    }
}

impl From<&PrivateOutputNote> for proto::transaction::PrivateOutputNote {
    fn from(note: &PrivateOutputNote) -> Self {
        Self {
            header: Some((*note.header()).into()),
            attachments: Some(note.attachments().into()),
        }
    }
}

impl From<PrivateOutputNote> for proto::transaction::PrivateOutputNote {
    fn from(note: PrivateOutputNote) -> Self {
        Self::from(&note)
    }
}

impl From<&OutputNote> for proto::transaction::OutputNote {
    fn from(note: &OutputNote) -> Self {
        use proto::transaction::output_note::Note;

        let note = match note {
            OutputNote::Public(note) => Note::Public(note.into()),
            OutputNote::Private(note) => Note::Private(note.into()),
        };
        Self { note: Some(note) }
    }
}

impl From<OutputNote> for proto::transaction::OutputNote {
    fn from(note: OutputNote) -> Self {
        Self::from(&note)
    }
}
