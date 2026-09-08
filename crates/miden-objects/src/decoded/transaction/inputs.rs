use miden_protobuf::unwrap_infallible;
pub use proto::transaction::DecodedForeignAccountSlotName as ForeignAccountSlotName;

use super::{InputNotesError, TransactionArgsError};
use crate::{Verify, proto};

#[cfg(test)]
mod tests;

impl Verify for ForeignAccountSlotName {
    type Verified =
        (miden_protocol::account::StorageSlotId, miden_protocol::account::StorageSlotName);
    type Error = ForeignAccountSlotNameError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let id = unwrap_infallible(self.slot_id.verify());
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

pub use proto::transaction::DecodedTransactionInputsV1 as TransactionInputsV1;

/// Checks input consistency and note inclusion against supplied headers, without authenticating the
/// chain.
impl crate::BuildUnchecked for TransactionInputsV1 {
    type Output = miden_protocol::transaction::TransactionInputs;
    type Error = TransactionInputsError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let account = self.account.verify()?;
        let header = self.block_header.build_unchecked()?;
        let config = self.protocol_config.verify()?;
        let chain = self.partial_blockchain.build_unchecked()?;
        let notes = self.input_notes.verify()?;
        let args = self.tx_args.verify()?;
        let advice = self.advice_inputs.verify()?;
        let code = self
            .foreign_account_code
            .into_iter()
            .map(Verify::verify)
            .collect::<Result<_, _>>()?;
        let mut names = alloc::collections::BTreeMap::new();
        for name in self.foreign_account_slot_names {
            let (id, name) = name.verify()?;
            if names.insert(id, name).is_some() {
                return Err(TransactionInputsError::DuplicateSlot(id));
            }
        }
        Ok(Self::Output::try_from_parts(
            account, header, config, chain, notes, args, advice, code, names,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionInputsError {
    #[error("{0}")]
    Account(#[from] crate::decoded::account::PartialAccountError),
    #[error("{0}")]
    Header(#[from] crate::decoded::blockchain::BlockHeaderError),
    #[error("{0}")]
    Config(#[from] crate::decoded::protocol_config::VerificationError),
    #[error("{0}")]
    Chain(#[from] crate::decoded::blockchain::PartialBlockchainError),
    #[error("{0}")]
    Notes(#[from] InputNotesError),
    #[error("{0}")]
    Args(#[from] TransactionArgsError),
    #[error("{0}")]
    Advice(#[from] crate::decoded::primitives::AdviceError),
    #[error("{0}")]
    Code(#[from] crate::decoded::account::AccountCodeError),
    #[error("{0}")]
    Slot(#[from] ForeignAccountSlotNameError),
    #[error("duplicate foreign account storage slot ID {0}")]
    DuplicateSlot(miden_protocol::account::StorageSlotId),
    #[error("{0}")]
    Inputs(#[from] miden_protocol::errors::TransactionInputError),
}

pub use proto::transaction::DecodedTransactionInputs as TransactionInputs;

/// Dispatches the decoded version without adding trust to the supplied headers or chain.
impl crate::BuildUnchecked for TransactionInputs {
    type Output = miden_protocol::transaction::TransactionInputs;
    type Error = TransactionInputsError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let proto::transaction::transaction_inputs::DecodedVersion::V1(inputs) = self.version;
        inputs.build_unchecked()
    }
}
