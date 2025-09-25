mod r#type;

mod interface;
mod network_id;
use alloc::string::{String, ToString};

use bech32::Bech32m;
use bech32::primitives::decode::{ByteIter, CheckedHrpstring};
pub use interface::AddressInterface;
use miden_core::utils::{ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;
pub use network_id::{CustomNetworkId, NetworkId};
pub use r#type::AddressType;

use crate::AddressError;
use crate::account::{AccountId, AccountStorageMode};
use crate::errors::Bech32Error;
use crate::note::NoteTag;

/// A user-facing address in Miden.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Address {
    AccountId(AccountIdAddress) = 0,
}

impl Address {
    /// Returns a note tag derived from this address.
    pub fn to_note_tag(&self) -> NoteTag {
        match self {
            Address::AccountId(addr) => addr.to_note_tag(),
        }
    }

    /// Returns the [`AddressInterface`] of the account to which the address points.
    pub fn interface(&self) -> AddressInterface {
        match self {
            Address::AccountId(account_id_address) => account_id_address.interface(),
        }
    }

    /// Encodes the [`Address`] into a [bech32](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki) string.
    ///
    /// ## Encoding
    ///
    /// The encoding of an address into bech32 is done as follows:
    /// - Encode the underlying address to bytes.
    /// - Into that data, insert the [`AddressType`] byte at index 0, shifting all other elements to
    ///   the right.
    /// - Choose an HRP, defined as a [`NetworkId`], e.g. [`NetworkId::Mainnet`] whose string
    ///   representation is `mm`.
    /// - Encode the resulting HRP together with the data into a bech32 string using the
    ///   [`bech32::Bech32m`] checksum algorithm.
    ///
    /// This is an example of an address in bech32 representation:
    ///
    /// ```text
    /// mm1qpkdyek2c0ywwvzupakc7zlzty8qn2qnfc
    /// ```
    ///
    /// ## Rationale
    ///
    /// The address type is at the very beginning so that it can be decoded first to detect the type
    /// of the address, without having to decode the entire data. Moreover, since the address type
    /// is chosen as a multiple of 8, the first character of the bech32 string after the
    /// `1` separator will be different for every address type. That makes the type of the address
    /// conveniently human-readable.
    ///
    /// The only allowed checksum algorithm is [`Bech32m`] due to being the best available checksum
    /// algorithm with no known weaknesses (unlike [`Bech32`](bech32::Bech32)). No checksum is
    /// also not allowed since the intended use of bech32 is to have error
    /// detection capabilities.
    pub fn to_bech32(&self, network_id: NetworkId) -> String {
        match self {
            Address::AccountId(account_id_address) => account_id_address.to_bech32(network_id),
        }
    }

    /// Decodes a [bech32](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki) string
    /// into the [`NetworkId`] and an [`Address`].
    ///
    /// See [`Address::to_bech32`] for details on the format. The procedure for decoding the bech32
    /// data into the address are the inverse operations of encoding.
    pub fn from_bech32(bech32_string: &str) -> Result<(NetworkId, Self), AddressError> {
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

        // We only know the expected length once we know the address type, but to get the address
        // type, the length must be at least one.
        let address_byte = byte_iter.next().ok_or_else(|| {
            AddressError::Bech32DecodeError(Bech32Error::InvalidDataLength {
                expected: 1,
                actual: byte_iter.len(),
            })
        })?;

        let address_type = AddressType::try_from(address_byte)?;

        let address = match address_type {
            AddressType::AccountId => {
                AccountIdAddress::from_bech32_byte_iter(byte_iter).map(Address::from)?
            },
        };

        Ok((network_id, address))
    }

    /// Identifier for the internal type of address
    fn address_scheme_id(&self) -> u8 {
        match self {
            Address::AccountId(_) => 0u8,
        }
    }
}

impl Serializable for Address {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.address_scheme_id());
        match self {
            Address::AccountId(addr) => {
                let serialized: [u8; AccountIdAddress::SERIALIZED_SIZE] = (*addr).into();
                serialized.write_into(target);
            },
        }
    }
}

impl Deserializable for Address {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, DeserializationError> {
        let address_scheme_id: u8 = source.read_u8()?;
        match address_scheme_id {
            // AccountIdAddress
            0u8 => {
                let bytes: [u8; AccountIdAddress::SERIALIZED_SIZE] = source.read_array()?;
                let account_id_address = AccountIdAddress::try_from(bytes)
                    .map_err(|err| DeserializationError::InvalidValue(format!("{}", err)))?;
                Ok(Address::AccountId(account_id_address))
            },
            val => {
                Err(DeserializationError::InvalidValue(format!("Invalid address scheme ID {val}")))
            },
        }
    }
}

// ACCOUNT ID ADDRESS
// ================================================================================================

/// An [`Address`] that targets a specific [`AccountId`] with an explicit tag length preference.
///
/// The tag length preference determines how many bits of the account ID are encoded into
/// [`NoteTag`]s of notes targeted to this address. This lets the owner of the account choose
/// their level of privacy. A higher tag length makes the account more uniquely identifiable and
/// reduces privacy, while a shorter length increases privacy at the cost of matching more notes
/// published onchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountIdAddress {
    id: AccountId,
    tag_len: u8,
    interface: AddressInterface,
}

impl AccountIdAddress {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The serialized size of an [`AccountIdAddress`] in bytes.
    pub const SERIALIZED_SIZE: usize = AccountId::SERIALIZED_SIZE + 2;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new account ID based address with the default tag length.
    ///
    /// The tag length defaults to [`NoteTag::DEFAULT_LOCAL_TAG_LENGTH`] for local, and
    /// [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`] for network accounts.
    pub fn new(id: AccountId, interface: AddressInterface) -> Self {
        let tag_len = if id.storage_mode() == AccountStorageMode::Network {
            NoteTag::DEFAULT_NETWORK_TAG_LENGTH
        } else {
            NoteTag::DEFAULT_LOCAL_TAG_LENGTH
        };

        Self { id, tag_len, interface }
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Sets a custom tag length for the address, determining how many bits of the account ID
    /// are encoded into [`NoteTag`]s.
    ///
    /// For local (both public and private) accounts, up to 30 bits can be encoded into the tag.
    /// For network accounts, the tag length must be set to 30 bits.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tag length exceeds [`NoteTag::MAX_LOCAL_TAG_LENGTH`] for local accounts.
    /// - The tag length is not [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`] for network accounts.
    pub fn with_tag_len(mut self, tag_len: u8) -> Result<Self, AddressError> {
        if self.id.storage_mode() == AccountStorageMode::Network {
            if tag_len != NoteTag::DEFAULT_NETWORK_TAG_LENGTH {
                return Err(AddressError::CustomTagLengthNotAllowedForNetworkAccounts(tag_len));
            }
        } else if tag_len > NoteTag::MAX_LOCAL_TAG_LENGTH {
            return Err(AddressError::TagLengthTooLarge(tag_len));
        }

        self.tag_len = tag_len;
        Ok(self)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the underlying account id.
    pub fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the preferred tag length.
    ///
    /// This is guaranteed to be in range `0..=30` (e.g. the maximum of
    /// [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`]).
    pub fn note_tag_len(&self) -> u8 {
        self.tag_len
    }

    /// Returns the [`AddressInterface`] of the account to which the address points.
    pub fn interface(&self) -> AddressInterface {
        self.interface
    }

    /// Returns a note tag derived from this address.
    pub fn to_note_tag(&self) -> NoteTag {
        match self.id.storage_mode() {
            AccountStorageMode::Network => NoteTag::from_network_account_id(self.id),
            AccountStorageMode::Private | AccountStorageMode::Public => {
                NoteTag::from_local_account_id(self.id, self.tag_len)
                    .expect("AccountIdAddress validated that tag len does not exceed MAX_LOCAL_TAG_LENGTH bits")
            },
        }
    }

    // PRIVATE HELPERS
    // ----------------------------------------------------------------------------------------

    /// Encodes the [`AccountIdAddress`] to a bech32 string.
    ///
    /// See [`Address::to_bech32`] for more details.
    fn to_bech32(self, network_id: NetworkId) -> String {
        let id_bytes: [u8; Self::SERIALIZED_SIZE] = self.into();

        // Create an array that fits the encoded account ID address plus the address type byte.
        let mut data = [0; Self::SERIALIZED_SIZE + 1];
        // Encode the address type into index 0.
        data[0] = AddressType::AccountId as u8;
        // Encode the 17 account ID address bytes into 1..18.
        data[1..].copy_from_slice(&id_bytes);

        // SAFETY: Encoding panics if the total length of the hrp + data (encoded in GF(32)) + the
        // separator + the checksum exceeds Bech32m::CODE_LENGTH, which is 1023.
        // The total 18 bytes of data we encode result in (18 bytes * 8 bits / 5 bits per base32
        // symbol) = 29 characters. The hrp is at most 83 in length, so we are guaranteed to be
        // below the limit.
        bech32::encode::<Bech32m>(network_id.into_hrp(), &data)
            .expect("code length of bech32 should not be exceeded")
    }

    /// Decodes the data from the bech32 byte iterator into an [`AccountIdAddress`].
    ///
    /// See [`Address::from_bech32`] for details.
    fn from_bech32_byte_iter(byte_iter: ByteIter<'_>) -> Result<Self, AddressError> {
        // The _remaining_ length of the iterator must be the serialized size of the account ID
        // address.
        if byte_iter.len() != Self::SERIALIZED_SIZE {
            return Err(AddressError::Bech32DecodeError(Bech32Error::InvalidDataLength {
                expected: Self::SERIALIZED_SIZE,
                actual: byte_iter.len(),
            }));
        }

        // Every byte is guaranteed to be overwritten since we've checked the length of the
        // iterator.
        let mut id_bytes = [0_u8; Self::SERIALIZED_SIZE];
        for (i, byte) in byte_iter.enumerate() {
            id_bytes[i] = byte;
        }

        let account_id_address = Self::try_from(id_bytes)?;

        Ok(account_id_address)
    }
}

impl From<AccountIdAddress> for Address {
    fn from(addr: AccountIdAddress) -> Self {
        Address::AccountId(addr)
    }
}

impl From<AccountIdAddress> for [u8; AccountIdAddress::SERIALIZED_SIZE] {
    fn from(account_id_address: AccountIdAddress) -> Self {
        let mut result = [0_u8; AccountIdAddress::SERIALIZED_SIZE];

        // Encode the account ID into 0..15.
        let encoded_account_id_address = <[u8; 15]>::from(account_id_address.id);
        result[..15].copy_from_slice(&encoded_account_id_address);

        let interface = account_id_address.interface as u16;
        debug_assert_eq!(
            interface >> 11,
            0,
            "address interface should have its upper 5 bits unset"
        );

        // The interface takes up 11 bits and the tag length 5 bits, so we can merge them together.
        let tag_len = (account_id_address.tag_len as u16) << 11;
        let encoded = tag_len | interface;
        let encoded: [u8; 2] = encoded.to_be_bytes();

        // Encode the interface and tag length into 15..17.
        result[15] = encoded[0];
        result[16] = encoded[1];

        result
    }
}

impl TryFrom<[u8; AccountIdAddress::SERIALIZED_SIZE]> for AccountIdAddress {
    type Error = AddressError;

    fn try_from(bytes: [u8; AccountIdAddress::SERIALIZED_SIZE]) -> Result<Self, Self::Error> {
        let account_id_bytes: [u8; AccountId::SERIALIZED_SIZE] = bytes
            [..AccountId::SERIALIZED_SIZE]
            .try_into()
            .expect("we should have sliced off exactly 15 bytes");
        let account_id =
            AccountId::try_from(account_id_bytes).map_err(AddressError::AccountIdDecodeError)?;

        let interface_tag_len = u16::from_be_bytes([bytes[15], bytes[16]]);
        let tag_len = (interface_tag_len >> 11) as u8;
        let interface = interface_tag_len & 0b0000_0111_1111_1111;
        let interface = AddressInterface::try_from(interface)?;

        Self::new(account_id, interface).with_tag_len(tag_len)
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::str::FromStr;

    use assert_matches::assert_matches;
    use bech32::{Bech32, NoChecksum};

    use super::*;
    use crate::account::AccountType;
    use crate::address::CustomNetworkId;
    use crate::testing::account_id::{ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, AccountIdBuilder};

    /// Tests that an account ID address can be encoded and decoded.
    #[test]
    fn address_bech32_encode_decode_roundtrip() {
        // We use this to check that encoding does not panic even when using the longest possible
        // HRP.
        let longest_possible_hrp =
            "01234567890123456789012345678901234567890123456789012345678901234567890123456789012";
        assert_eq!(longest_possible_hrp.len(), 83);

        let rng = &mut rand::rng();
        for network_id in [
            NetworkId::Mainnet,
            NetworkId::Custom(Box::new(CustomNetworkId::from_str("custom").unwrap())),
            NetworkId::Custom(Box::new(CustomNetworkId::from_str(longest_possible_hrp).unwrap())),
        ] {
            for (idx, account_id) in [
                AccountIdBuilder::new()
                    .account_type(AccountType::FungibleFaucet)
                    .build_with_rng(rng),
                AccountIdBuilder::new()
                    .account_type(AccountType::NonFungibleFaucet)
                    .build_with_rng(rng),
                AccountIdBuilder::new()
                    .account_type(AccountType::RegularAccountImmutableCode)
                    .build_with_rng(rng),
                AccountIdBuilder::new()
                    .account_type(AccountType::RegularAccountUpdatableCode)
                    .build_with_rng(rng),
            ]
            .into_iter()
            .enumerate()
            {
                let account_id_address =
                    AccountIdAddress::new(account_id, AddressInterface::BasicWallet);
                let address = Address::from(account_id_address);

                let bech32_string = address.to_bech32(network_id.clone());
                let (decoded_network_id, decoded_address) =
                    Address::from_bech32(&bech32_string).unwrap();

                assert_eq!(network_id, decoded_network_id, "network id failed in {idx}");
                assert_eq!(address, decoded_address, "address failed in {idx}");

                let Address::AccountId(decoded_account_id) = address;
                assert_eq!(account_id, decoded_account_id.id());
                assert_eq!(account_id_address.note_tag_len(), decoded_account_id.note_tag_len());
            }
        }
    }

    /// Tests that an invalid checksum returns an error.
    #[test]
    fn bech32_invalid_checksum() {
        let network_id = NetworkId::Mainnet;
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let address =
            Address::from(AccountIdAddress::new(account_id, AddressInterface::BasicWallet));

        let bech32_string = address.to_bech32(network_id);
        let mut invalid_bech32_1 = bech32_string.clone();
        invalid_bech32_1.remove(0);
        let mut invalid_bech32_2 = bech32_string.clone();
        invalid_bech32_2.remove(7);

        let error = Address::from_bech32(&invalid_bech32_1).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));

        let error = Address::from_bech32(&invalid_bech32_2).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));
    }

    /// Tests that an unknown address type returns an error.
    #[test]
    fn bech32_unknown_address_type() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let account_id_address = AccountIdAddress::new(account_id, AddressInterface::BasicWallet);
        let mut id_address_bytes = <[u8; _]>::from(account_id_address).to_vec();

        // Set invalid address type.
        id_address_bytes.insert(0, 250);

        let invalid_bech32 =
            bech32::encode::<Bech32m>(NetworkId::Mainnet.into_hrp(), &id_address_bytes).unwrap();

        let error = Address::from_bech32(&invalid_bech32).unwrap_err();
        assert_matches!(
            error,
            AddressError::Bech32DecodeError(Bech32Error::UnknownAddressType(250))
        );
    }

    /// Tests that a bech32 using a disallowed checksum returns an error.
    #[test]
    fn bech32_invalid_other_checksum() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let account_id_address = AccountIdAddress::new(account_id, AddressInterface::BasicWallet);
        let mut id_address_bytes = <[u8; _]>::from(account_id_address).to_vec();
        id_address_bytes.insert(0, AddressType::AccountId as u8);

        // Use Bech32 instead of Bech32m which is disallowed.
        let invalid_bech32_regular =
            bech32::encode::<Bech32>(NetworkId::Mainnet.into_hrp(), &id_address_bytes).unwrap();
        let error = Address::from_bech32(&invalid_bech32_regular).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));

        // Use no checksum instead of Bech32m which is disallowed.
        let invalid_bech32_no_checksum =
            bech32::encode::<NoChecksum>(NetworkId::Mainnet.into_hrp(), &id_address_bytes).unwrap();
        let error = Address::from_bech32(&invalid_bech32_no_checksum).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));
    }

    /// Tests that a bech32 string encoding data of an unexpected length returns an error.
    #[test]
    fn bech32_invalid_length() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let account_id_address = AccountIdAddress::new(account_id, AddressInterface::BasicWallet);
        let mut id_address_bytes = <[u8; _]>::from(account_id_address).to_vec();
        id_address_bytes.insert(0, AddressType::AccountId as u8);
        // Add one byte to make the length invalid.
        id_address_bytes.push(5);

        let invalid_bech32 =
            bech32::encode::<Bech32m>(NetworkId::Mainnet.into_hrp(), &id_address_bytes).unwrap();

        let error = Address::from_bech32(&invalid_bech32).unwrap_err();
        assert_matches!(
            error,
            AddressError::Bech32DecodeError(Bech32Error::InvalidDataLength { .. })
        );
    }

    /// Tests that an Address can be serialized and deserialized
    #[test]
    fn address_serialization() {
        let rng = &mut rand::rng();

        for account_id in [
            AccountIdBuilder::new()
                .account_type(AccountType::FungibleFaucet)
                .build_with_rng(rng),
            AccountIdBuilder::new()
                .account_type(AccountType::NonFungibleFaucet)
                .build_with_rng(rng),
            AccountIdBuilder::new()
                .account_type(AccountType::RegularAccountImmutableCode)
                .build_with_rng(rng),
            AccountIdBuilder::new()
                .account_type(AccountType::RegularAccountUpdatableCode)
                .build_with_rng(rng),
        ]
        .into_iter()
        {
            for account_id_address in [
                AccountIdAddress::new(account_id, AddressInterface::BasicWallet),
                AccountIdAddress::new(account_id, AddressInterface::Unspecified),
            ] {
                let address = Address::from(account_id_address);
                let serialized = address.to_bytes();
                let deserialized = Address::read_from_bytes(&serialized).unwrap();
                assert_eq!(address, deserialized);
            }
        }
    }
}
