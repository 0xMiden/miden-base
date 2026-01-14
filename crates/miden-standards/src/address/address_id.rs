use alloc::string::ToString;
use alloc::vec::Vec;

use bech32::Bech32m;
use bech32::primitives::decode::CheckedHrpstring;
use miden_processor::DeserializationError;
use miden_protocol::AddressError;
use miden_protocol::account::{AccountId, AccountStorageMode};
use miden_protocol::address::{AddressType, NetworkId};
use miden_protocol::errors::Bech32Error;
use miden_protocol::note::NoteTag;
use miden_protocol::utils::serde::{ByteWriter, Deserializable, Serializable};

/// The identifier of an [`Address`](super::Address).
///
/// See the address docs for more details.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressId {
    AccountId(AccountId),
}

impl AddressId {
    /// Returns the [`AddressType`] of this ID.
    pub fn address_type(&self) -> AddressType {
        match self {
            AddressId::AccountId(_) => AddressType::AccountId,
        }
    }

    /// Returns the default tag length of the ID.
    ///
    /// This is guaranteed to be in range `0..=30` (e.g. the maximum of
    /// [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`]).
    pub fn default_note_tag_len(&self) -> u8 {
        match self {
            AddressId::AccountId(id) => {
                if id.storage_mode() == AccountStorageMode::Network {
                    NoteTag::DEFAULT_NETWORK_TAG_LENGTH
                } else {
                    NoteTag::DEFAULT_LOCAL_TAG_LENGTH
                }
            },
        }
    }

    /// Returns the bytes representation of this address ID.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.write_into(&mut bytes);
        bytes
    }

    /// Decodes a bech32 string into an identifier.
    pub(crate) fn decode(bech32_string: &str) -> Result<(NetworkId, Self), AddressError> {
        // We use CheckedHrpString with an explicit checksum algorithm so we don't allow the
        // `Bech32` or `NoChecksum` algorithms.
        let checked_string = CheckedHrpstring::new::<Bech32m>(bech32_string).map_err(|source| {
            // The CheckedHrpStringError does not implement core::error::Error, only
            // std::error::Error, so for now we convert it to a String. Even if it will
            // implement the trait in the future, we should include it as an opaque
            // error since the crate does not have a stable release yet.
            AddressError::Bech32DecodeError(Bech32Error::DecodeError(source.to_string().into()))
        })?;

        let hrp = checked_string.hrp();
        let network_id = NetworkId::from_hrp(hrp);

        let mut byte_iter = checked_string.byte_iter();

        // We only know the expected length once we know the address type, but to get the
        // address type, the length must be at least one.
        let address_byte = byte_iter.next().ok_or_else(|| {
            AddressError::Bech32DecodeError(Bech32Error::InvalidDataLength {
                expected: 1,
                actual: byte_iter.len(),
            })
        })?;

        let address_type = AddressType::try_from(address_byte)?;

        // Collect the remaining bytes into a Vec to convert from ByteIter to a regular iterator
        let remaining_bytes: Vec<u8> = byte_iter.collect();

        let identifier = match address_type {
            AddressType::AccountId => AccountId::from_bech32_byte_iter(remaining_bytes.into_iter())
                .map_err(AddressError::AccountIdDecodeError)
                .map(AddressId::AccountId)?,
            // AddressType is non-exhaustive, so we need a catch-all
            _ => {
                return Err(AddressError::Bech32DecodeError(
                    miden_protocol::errors::Bech32Error::UnknownAddressType(address_type as u8),
                ));
            },
        };

        Ok((network_id, identifier))
    }
}

impl From<AccountId> for AddressId {
    fn from(id: AccountId) -> Self {
        Self::AccountId(id)
    }
}

impl Serializable for AddressId {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.address_type() as u8);
        match self {
            AddressId::AccountId(id) => {
                id.write_into(target);
            },
        }
    }
}

impl Deserializable for AddressId {
    fn read_from<R: miden_protocol::utils::serde::ByteReader>(
        source: &mut R,
    ) -> Result<Self, DeserializationError> {
        let address_type: u8 = source.read_u8()?;
        let address_type = AddressType::try_from(address_type)
            .map_err(|err| DeserializationError::InvalidValue(format!("{}", err)))?;

        match address_type {
            AddressType::AccountId => {
                let id: AccountId = source.read()?;
                Ok(AddressId::AccountId(id))
            },
            // AddressType is non-exhaustive, so we need a catch-all
            _ => Err(DeserializationError::InvalidValue(format!(
                "unknown address type: {}",
                address_type as u8
            ))),
        }
    }
}
