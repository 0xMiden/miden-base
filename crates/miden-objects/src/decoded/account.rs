//! Domain construction for decoded account messages.
pub use proto::account::DecodedStorageSlotId as StorageSlotId;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for StorageSlotId {
    type Verified = miden_protocol::account::StorageSlotId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.suffix, self.prefix))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::StorageSlotId> for miden_protocol::account::StorageSlotId {
    type Error = ConversionError;
    fn try_from(value: proto::account::StorageSlotId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::account::DecodedStorageMapEntry as StorageMapEntry;

impl Verify for StorageMapEntry {
    type Verified = (miden_protocol::account::StorageMapKey, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((miden_protocol::account::StorageMapKey::from_raw(self.key), self.value))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::StorageMapEntry>
    for (miden_protocol::account::StorageMapKey, miden_protocol::Word)
{
    type Error = ConversionError;
    fn try_from(value: proto::account::StorageMapEntry) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::account::DecodedAccountCode as AccountCode;

impl Verify for AccountCode {
    type Verified = miden_protocol::account::AccountCode;
    type Error = miden_protocol::errors::AccountError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let roots = self
            .procedure_roots
            .into_iter()
            .map(miden_protocol::account::AccountProcedureRoot::from_raw)
            .collect();
        Self::Verified::from_parts(alloc::sync::Arc::new(self.mast), roots)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::AccountCode> for miden_protocol::account::AccountCode {
    type Error = ConversionError;
    fn try_from(value: proto::account::AccountCode) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::account::DecodedAccountWitness as AccountWitness;

impl Verify for AccountWitness {
    type Verified = miden_protocol::block::account_tree::AccountWitness;
    type Error = AccountWitnessError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.witness_id, self.commitment, self.path.verify()?)?)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AccountWitnessError {
    #[error("invalid witness path: {0}")]
    Path(#[from] miden_protocol::crypto::merkle::MerkleError),
    #[error("invalid account witness: {0}")]
    Witness(#[from] miden_protocol::errors::AccountTreeError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::AccountWitness>
    for miden_protocol::block::account_tree::AccountWitness
{
    type Error = ConversionError;
    fn try_from(value: proto::account::AccountWitness) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::account::DecodedAccountVaultPatchEntry as AccountVaultPatchEntry;

impl Verify for AccountVaultPatchEntry {
    type Verified = (miden_protocol::asset::AssetId, miden_protocol::Word);
    type Error = miden_protocol::errors::AssetError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.asset_id.try_into()?, self.value))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::AccountVaultPatchEntry>
    for (miden_protocol::asset::AssetId, miden_protocol::Word)
{
    type Error = ConversionError;
    fn try_from(value: proto::account::AccountVaultPatchEntry) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}
