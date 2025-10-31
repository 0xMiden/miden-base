mod r#type;
use alloc::string::ToString;

pub use r#type::AddressType;

mod routing_parameters;
use alloc::borrow::ToOwned;

pub use routing_parameters::RoutingParameters;

mod interface;
mod network_id;
use alloc::string::String;

pub use interface::AddressInterface;
use miden_processor::DeserializationError;
pub use network_id::{CustomNetworkId, NetworkId};

use crate::AddressError;
use crate::account::AccountStorageMode;
use crate::note::NoteTag;
use crate::utils::serde::{ByteWriter, Deserializable, Serializable};

mod address_id;
pub use address_id::AddressId;

const ADDRESS_SEPARATOR: char = '_';

/// A user-facing address in Miden.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    id: AddressId,
    routing_params: RoutingParameters,
}

impl Address {
    pub fn new(id: impl Into<AddressId>) -> Self {
        Self {
            id: id.into(),
            routing_params: RoutingParameters::default(),
        }
    }

    pub fn with_routing_parameters(
        mut self,
        routing_params: RoutingParameters,
    ) -> Result<Self, AddressError> {
        if let Some(tag_len) = routing_params.tag_len() {
            match self.id {
                AddressId::AccountId(account_id) => {
                    if account_id.storage_mode() == AccountStorageMode::Network {
                        if tag_len != NoteTag::DEFAULT_NETWORK_TAG_LENGTH {
                            return Err(AddressError::CustomTagLengthNotAllowedForNetworkAccounts(
                                tag_len,
                            ));
                        }
                    } else if tag_len > NoteTag::MAX_LOCAL_TAG_LENGTH {
                        return Err(AddressError::TagLengthTooLarge(tag_len));
                    }
                },
            }
        }

        self.routing_params = routing_params;

        Ok(self)
    }

    pub fn id(&self) -> AddressId {
        self.id
    }

    /// Returns the [`AddressInterface`] of the account to which the address points.
    pub fn interface(&self) -> Option<AddressInterface> {
        self.routing_params.interface()
    }

    /// Returns the preferred tag length.
    ///
    /// This is guaranteed to be in range `0..=30` (e.g. the maximum of
    /// [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`]).
    pub fn note_tag_len(&self) -> u8 {
        match self.id {
            AddressId::AccountId(id) => self.routing_params.tag_len().unwrap_or_else(|| {
                if id.storage_mode() == AccountStorageMode::Network {
                    NoteTag::DEFAULT_NETWORK_TAG_LENGTH
                } else {
                    NoteTag::DEFAULT_LOCAL_TAG_LENGTH
                }
            }),
        }
    }

    /// Returns a note tag derived from this address.
    pub fn to_note_tag(&self) -> NoteTag {
        let note_tag_len = self.note_tag_len();

        match self.id {
            AddressId::AccountId(id) => {
                match id.storage_mode() {
                  AccountStorageMode::Network => NoteTag::from_network_account_id(id),
                  AccountStorageMode::Private | AccountStorageMode::Public => {
                      NoteTag::from_local_account_id(id, note_tag_len)
                          .expect("AccountIdAddress validated that tag len does not exceed MAX_LOCAL_TAG_LENGTH bits")
                    }
                }
            },
        }
    }

    /// TODO: This is outdated - update.
    ///
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
    pub fn encode(&self, network_id: NetworkId) -> String {
        let mut encoded = match self.id {
            AddressId::AccountId(id) => id.to_bech32(network_id),
        };

        encoded.push(ADDRESS_SEPARATOR);
        encoded.push_str(&self.routing_params.encode_to_string());

        encoded
    }

    /// Decodes an address string into the [`NetworkId`] and an [`Address`].
    ///
    /// See [`Address::encode`] for details on the format. The procedure for decoding the bech32
    /// data into the address are the inverse operations of encoding.
    pub fn decode(address_str: &str) -> Result<(NetworkId, Self), AddressError> {
        if address_str.ends_with(ADDRESS_SEPARATOR) {
            return Err(AddressError::TrailingSeparator);
        }

        let mut split = address_str.split(ADDRESS_SEPARATOR);
        let encoded_identifier = split
            .next()
            .ok_or_else(|| AddressError::decode_error("identifier missing in address string"))?;

        let (network_id, identifier) = AddressId::decode(encoded_identifier)?;

        let routing_params = if let Some(encoded_routing_params) = split.next() {
            RoutingParameters::decode(encoded_routing_params.to_owned())?
        } else {
            RoutingParameters::default()
        };

        let address = Address::new(identifier).with_routing_parameters(routing_params)?;

        Ok((network_id, address))
    }
}

impl Serializable for Address {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.id.write_into(target);
        self.routing_params.write_into(target);
    }
}

impl Deserializable for Address {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, DeserializationError> {
        let identifier: AddressId = source.read()?;
        let routing_params = source.read()?;
        Self::new(identifier)
            .with_routing_parameters(routing_params)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::str::FromStr;

    use assert_matches::assert_matches;
    use bech32::{Bech32, Bech32m, NoChecksum};

    use super::*;
    use crate::AccountIdError;
    use crate::account::{AccountId, AccountType};
    use crate::address::CustomNetworkId;
    use crate::errors::Bech32Error;
    use crate::testing::account_id::{ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, AccountIdBuilder};

    /// Tests that an account ID address can be encoded and decoded.
    #[test]
    fn address_encode_decode_roundtrip() -> anyhow::Result<()> {
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
                let address = Address::new(account_id).with_routing_parameters(
                    RoutingParameters::new().with_receiver_profile(
                        NoteTag::DEFAULT_NETWORK_TAG_LENGTH,
                        AddressInterface::BasicWallet,
                    ),
                )?;

                let bech32_string = address.encode(network_id.clone());
                let (decoded_network_id, decoded_address) = Address::decode(&bech32_string)?;

                assert_eq!(network_id, decoded_network_id, "network id failed in {idx}");
                assert_eq!(address, decoded_address, "address failed in {idx}");

                let AddressId::AccountId(decoded_account_id) = address.id();
                assert_eq!(account_id, decoded_account_id);

                // It should always be possible to strip the routing parameters and still have a
                // valid address ID.
                let address_id_str = bech32_string.split(ADDRESS_SEPARATOR).next().unwrap();
                let (decoded_network_id, decoded_address) = Address::decode(address_id_str)?;

                assert_eq!(network_id, decoded_network_id, "network id failed in {idx}");
                assert_eq!(address.id(), decoded_address.id(), "address ID failed in {idx}");
            }
        }

        Ok(())
    }

    #[test]
    fn address_fails_on_trailing_separator() -> anyhow::Result<()> {
        let id = AccountIdBuilder::new()
            .account_type(AccountType::FungibleFaucet)
            .build_with_rng(&mut rand::rng());

        let address = Address::new(id);
        let mut encoded_address = address.encode(NetworkId::Devnet);
        encoded_address.push(ADDRESS_SEPARATOR);

        let err = Address::decode(&encoded_address).unwrap_err();
        assert_matches!(err, AddressError::TrailingSeparator);

        Ok(())
    }

    /// Tests that an invalid checksum returns an error.
    #[test]
    fn bech32_invalid_checksum() -> anyhow::Result<()> {
        let network_id = NetworkId::Mainnet;
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
        let address = Address::new(account_id).with_routing_parameters(
            RoutingParameters::new().with_receiver_profile(14, AddressInterface::BasicWallet),
        )?;

        let bech32_string = address.encode(network_id);
        let mut invalid_bech32_1 = bech32_string.clone();
        invalid_bech32_1.remove(0);
        let mut invalid_bech32_2 = bech32_string.clone();
        invalid_bech32_2.remove(7);

        let error = Address::decode(&invalid_bech32_1).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));

        let error = Address::decode(&invalid_bech32_2).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));

        Ok(())
    }

    /// Tests that an unknown address type returns an error.
    #[test]
    fn bech32_unknown_address_type() {
        let invalid_bech32_address =
            bech32::encode::<Bech32m>(NetworkId::Mainnet.into_hrp(), &[250]).unwrap();

        let error = Address::decode(&invalid_bech32_address).unwrap_err();
        assert_matches!(
            error,
            AddressError::Bech32DecodeError(Bech32Error::UnknownAddressType(250))
        );
    }

    /// Tests that a bech32 using a disallowed checksum returns an error.
    #[test]
    fn bech32_invalid_other_checksum() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let address_id_bytes = AddressId::from(account_id).to_bytes();

        // Use Bech32 instead of Bech32m which is disallowed.
        let invalid_bech32_regular =
            bech32::encode::<Bech32>(NetworkId::Mainnet.into_hrp(), &address_id_bytes).unwrap();
        let error = Address::decode(&invalid_bech32_regular).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));

        // Use no checksum instead of Bech32m which is disallowed.
        let invalid_bech32_no_checksum =
            bech32::encode::<NoChecksum>(NetworkId::Mainnet.into_hrp(), &address_id_bytes).unwrap();
        let error = Address::decode(&invalid_bech32_no_checksum).unwrap_err();
        assert_matches!(error, AddressError::Bech32DecodeError(Bech32Error::DecodeError(_)));
    }

    /// Tests that a bech32 string encoding data of an unexpected length returns an error.
    #[test]
    fn bech32_invalid_length() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let mut address_id_bytes = AddressId::from(account_id).to_bytes();
        // Add one byte to make the length invalid.
        address_id_bytes.push(5);

        let invalid_bech32 =
            bech32::encode::<Bech32m>(NetworkId::Mainnet.into_hrp(), &address_id_bytes).unwrap();

        let error = Address::decode(&invalid_bech32).unwrap_err();
        assert_matches!(
            error,
            AddressError::AccountIdDecodeError(AccountIdError::Bech32DecodeError(
                Bech32Error::InvalidDataLength { .. }
            ))
        );
    }

    /// Tests that an Address can be serialized and deserialized
    #[test]
    fn address_serialization() -> anyhow::Result<()> {
        let rng = &mut rand::rng();

        for account_type in [
            AccountType::FungibleFaucet,
            AccountType::NonFungibleFaucet,
            AccountType::RegularAccountImmutableCode,
            AccountType::RegularAccountUpdatableCode,
        ]
        .into_iter()
        {
            let account_id = AccountIdBuilder::new().account_type(account_type).build_with_rng(rng);
            let address = Address::new(account_id).with_routing_parameters(
                RoutingParameters::new().with_receiver_profile(
                    NoteTag::DEFAULT_NETWORK_TAG_LENGTH,
                    AddressInterface::BasicWallet,
                ),
            )?;

            let serialized = address.to_bytes();
            let deserialized = Address::read_from_bytes(&serialized)?;
            assert_eq!(address, deserialized);
        }

        Ok(())
    }
}
