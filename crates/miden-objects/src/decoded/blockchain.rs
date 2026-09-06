//! Domain construction for decoded blockchain messages.
pub use proto::blockchain::DecodedTrackedMmrLeaf as TrackedMmrLeaf;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for TrackedMmrLeaf {
    type Verified = (u64, miden_protocol::Word, alloc::vec::Vec<miden_protocol::Word>);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.position, self.leaf, self.path))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::TrackedMmrLeaf>
    for (u64, miden_protocol::Word, alloc::vec::Vec<miden_protocol::Word>)
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::TrackedMmrLeaf) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedBlockNumber as BlockNumber;

impl Verify for BlockNumber {
    type Verified = miden_protocol::block::BlockNumber;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(self.block_num.into())
    }
}

pub use proto::blockchain::DecodedFeeParameters as FeeParameters;

impl Verify for FeeParameters {
    type Verified = miden_protocol::block::FeeParameters;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.verification_base_fee))
    }
}

pub use proto::blockchain::DecodedNextProtocolConfig as NextProtocolConfig;

impl Verify for NextProtocolConfig {
    type Verified = miden_protocol::protocol_config::NextProtocolConfig;
    type Error = miden_protocol::errors::ProtocolConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let effective_from = self.effective_from.verify().expect("infallible block number");
        Self::Verified::new(effective_from, self.protocol_config)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::NextProtocolConfig>
    for miden_protocol::protocol_config::NextProtocolConfig
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::NextProtocolConfig) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedValidatorConfig as ValidatorConfig;

impl Verify for ValidatorConfig {
    type Verified = miden_protocol::block::ValidatorConfig;
    type Error = ValidatorConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let keys = self
            .keys
            .into_iter()
            .map(Verify::verify)
            .collect::<Result<_, _>>()
            .expect("canonical public keys");
        Ok(Self::Verified::new(keys, self.quorum.try_into()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidatorConfigError {
    #[error("quorum is out of range: {0}")]
    Quorum(#[from] core::num::TryFromIntError),
    #[error("{0}")]
    Config(#[from] miden_protocol::errors::ValidatorConfigError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::ValidatorConfig> for miden_protocol::block::ValidatorConfig {
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::ValidatorConfig) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedBlockHeader as BlockHeader;

/// Builds a header without validating its parent linkage, signatures, or protocol transition.
impl crate::BuildUnchecked for BlockHeader {
    type Output = miden_protocol::block::BlockHeader;
    type Error = BlockHeaderError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        if self.version != proto::blockchain::BlockVersion::V1 {
            return Err(BlockHeaderError::UnspecifiedVersion);
        }
        Ok(Self::Output::new(
            self.prev_block_commitment,
            self.block_num.verify().expect("infallible block number"),
            self.chain_commitment,
            self.account_root,
            self.nullifier_root,
            self.note_root,
            self.tx_commitment,
            self.validator_config.verify()?,
            self.fee_parameters.verify().expect("infallible fee parameters"),
            self.protocol_config_commitment,
            self.next_protocol_config.map(Verify::verify).transpose()?,
            self.timestamp,
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlockHeaderError {
    #[error("block header version is unspecified")]
    UnspecifiedVersion,
    #[error("{0}")]
    Validators(#[from] ValidatorConfigError),
    #[error("{0}")]
    Upgrade(#[from] miden_protocol::errors::ProtocolConfigError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::BlockHeader> for miden_protocol::block::BlockHeader {
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::BlockHeader) -> Result<Self, Self::Error> {
        crate::BuildUnchecked::build_unchecked(value.decode_fields()?).map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedPartialBlockchain as PartialBlockchain;

/// Checks MMR reconstruction and header membership, but not header parent linkage or a trusted
/// root.
impl crate::BuildUnchecked for PartialBlockchain {
    type Output = miden_protocol::transaction::PartialBlockchain;
    type Error = PartialBlockchainError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        use miden_protocol::crypto::merkle::MerklePath;
        use miden_protocol::crypto::merkle::mmr::{Forest, MmrPeaks, PartialMmr};

        let size = usize::try_from(self.forest)?;
        let peaks = MmrPeaks::new(Forest::new(size)?, self.peaks)?;
        let mut mmr = PartialMmr::from_peaks(peaks);
        let mut previous = None;
        for tracked in self.tracked_leaves {
            let position = usize::try_from(tracked.position)?;
            if position >= size {
                return Err(PartialBlockchainError::Position { position, size });
            }
            if previous.is_some_and(|previous| position <= previous) {
                return Err(PartialBlockchainError::LeafOrder);
            }
            previous = Some(position);
            mmr.track(position, tracked.leaf, &MerklePath::new(tracked.path))?;
        }
        let mut previous = None;
        let mut headers = alloc::vec::Vec::new();
        for header in self.block_headers {
            let header = header.build_unchecked()?;
            if previous.is_some_and(|previous| header.block_num() <= previous) {
                return Err(PartialBlockchainError::HeaderOrder);
            }
            previous = Some(header.block_num());
            headers.push(header);
        }
        Ok(Self::Output::new(mmr, headers)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PartialBlockchainError {
    #[error("{0}")]
    Size(#[from] core::num::TryFromIntError),
    #[error("{0}")]
    Forest(#[from] miden_protocol::utils::serde::DeserializationError),
    #[error("{0}")]
    Mmr(#[from] miden_protocol::crypto::merkle::mmr::MmrError),
    #[error("{0}")]
    Header(#[from] BlockHeaderError),
    #[error("{0}")]
    Chain(#[from] miden_protocol::errors::PartialBlockchainError),
    #[error("tracked leaf position {position} is outside forest of size {size}")]
    Position { position: usize, size: usize },
    #[error("tracked leaf positions must be unique and strictly increasing")]
    LeafOrder,
    #[error("block headers must be unique and ordered by ascending block number")]
    HeaderOrder,
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::PartialBlockchain>
    for miden_protocol::transaction::PartialBlockchain
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::PartialBlockchain) -> Result<Self, Self::Error> {
        crate::BuildUnchecked::build_unchecked(value.decode_fields()?).map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedBlockAccountUpdate as BlockAccountUpdate;

impl Verify for BlockAccountUpdate {
    type Verified = miden_protocol::block::BlockAccountUpdate;
    type Error = BlockAccountUpdateError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.account_id,
            self.final_state_commitment,
            self.details.verify()?,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BlockAccountUpdateError {
    #[error("{0}")]
    Details(#[from] super::account::AccountPatchError),
    #[error("{0}")]
    Update(#[from] miden_protocol::errors::BlockAccountUpdateError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::BlockAccountUpdate> for miden_protocol::block::BlockAccountUpdate {
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::BlockAccountUpdate) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedIndexedOutputNote as IndexedOutputNote;

impl Verify for IndexedOutputNote {
    type Verified = (usize, miden_protocol::transaction::OutputNote);
    type Error = IndexedOutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.note_index_in_batch.try_into()?, self.note.verify()?))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IndexedOutputNoteError {
    #[error("{0}")]
    Index(#[from] core::num::TryFromIntError),
    #[error("{0}")]
    Note(#[from] super::transaction::OutputNoteError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::IndexedOutputNote>
    for (usize, miden_protocol::transaction::OutputNote)
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::IndexedOutputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}
