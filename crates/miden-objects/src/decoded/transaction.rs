//! Domain construction for decoded transaction messages.
pub use proto::transaction::DecodedTransactionId as TransactionId;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for TransactionId {
    type Verified = miden_protocol::transaction::TransactionId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::from_raw(self.id))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionId> for miden_protocol::transaction::TransactionId {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTransactionScript as TransactionScript;

impl Verify for TransactionScript {
    type Verified = miden_protocol::transaction::TransactionScript;
    type Error = ScriptError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let entrypoint = miden_protocol::MastNodeId::from_u32_safe(self.entrypoint, &self.mast)?;
        Self::Verified::from_parts(alloc::sync::Arc::new(self.mast), entrypoint)
            .map_err(|error| ScriptError::Script(alloc::boxed::Box::new(error)))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("invalid script entrypoint: {0}")]
    Entrypoint(#[from] miden_protocol::utils::serde::DeserializationError),
    #[error("invalid transaction script: {0}")]
    // The constructor's error type is not publicly nameable.
    Script(#[source] alloc::boxed::Box<dyn core::error::Error + Send + Sync>),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionScript>
    for miden_protocol::transaction::TransactionScript
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionScript) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedNoteArgument as NoteArgument;

impl Verify for NoteArgument {
    type Verified = (miden_protocol::note::NoteId, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.note_id.verify()?, self.args))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::NoteArgument>
    for (miden_protocol::note::NoteId, miden_protocol::Word)
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::NoteArgument) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTransactionArgs as TransactionArgs;

impl Verify for TransactionArgs {
    type Verified = miden_protocol::transaction::TransactionArgs;
    type Error = TransactionArgsError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let tx_script = self.tx_script.map(Verify::verify).transpose()?;
        let mut note_args = alloc::collections::BTreeMap::new();
        for argument in self.note_args {
            let (id, args) = argument.verify().expect("infallible note argument");
            if note_args.insert(id, args).is_some() {
                return Err(TransactionArgsError::DuplicateNoteArgument(id));
            }
        }
        Ok(Self::Verified::from_parts(
            tx_script,
            self.tx_script_args,
            note_args,
            self.advice_inputs.verify()?,
            self.auth_args,
        ))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TransactionArgsError {
    #[error("invalid transaction script: {0}")]
    Script(#[from] ScriptError),
    #[error("invalid advice inputs: {0}")]
    Advice(#[from] super::primitives::AdviceError),
    #[error("duplicate note argument {0}")]
    DuplicateNoteArgument(miden_protocol::note::NoteId),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionArgs> for miden_protocol::transaction::TransactionArgs {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionArgs) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedForeignAccountSlotName as ForeignAccountSlotName;

impl Verify for ForeignAccountSlotName {
    type Verified =
        (miden_protocol::account::StorageSlotId, miden_protocol::account::StorageSlotName);
    type Error = ForeignAccountSlotNameError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let id = self.slot_id.verify().expect("infallible storage slot ID");
        let name = miden_protocol::account::StorageSlotName::new(self.slot_name)?;
        if name.id() != id {
            return Err(ForeignAccountSlotNameError::IdMismatch {
                expected: name.id(),
                actual: id,
            });
        }
        Ok((id, name))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ForeignAccountSlotNameError {
    #[error("invalid storage slot name: {0}")]
    Name(#[from] miden_protocol::errors::StorageSlotNameError),
    #[error("storage slot ID {actual} does not match the name's ID {expected}")]
    IdMismatch {
        expected: miden_protocol::account::StorageSlotId,
        actual: miden_protocol::account::StorageSlotId,
    },
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::ForeignAccountSlotName>
    for (miden_protocol::account::StorageSlotId, miden_protocol::account::StorageSlotName)
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::ForeignAccountSlotName) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}
