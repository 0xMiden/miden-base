use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bech32::primitives::decode::CheckedHrpstring;
use bech32::{Bech32m, Hrp};

use crate::AddressError;
use crate::address::AddressInterface;
use crate::errors::Bech32Error;
use crate::utils::sync::LazyLock;

/// The HRP used for encoding routing parameters.
///
/// `mrp` stands for Miden Routing Parameters.
static ROUTING_PARAMETERS_HRP: LazyLock<Hrp> =
    LazyLock::new(|| Hrp::parse("mrp").expect("hrp should be valid"));

const BECH32_SEPARATOR: &str = "1";

const RECEIVER_PROFILE_KEY: u8 = 0;

/// TODO: Docs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutingParameters {
    receiver_profile: Option<(u8, AddressInterface)>,
}

impl RoutingParameters {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    pub fn new() -> Self {
        Self { receiver_profile: None }
    }

    pub fn with_receiver_profile(mut self, tag_len: u8, interface: AddressInterface) -> Self {
        self.receiver_profile = Some((tag_len, interface));
        self
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    pub fn receiver_profile(&self) -> Option<(u8, AddressInterface)> {
        self.receiver_profile
    }

    pub fn tag_len(&self) -> Option<u8> {
        self.receiver_profile.map(|(tag_len, _interface)| tag_len)
    }

    pub fn interface(&self) -> Option<AddressInterface> {
        self.receiver_profile.map(|(_tag_len, interface)| interface)
    }

    /// Encodes [`RoutingParameters`] to a bech32 string _without_ the leading hrp and separator.
    ///
    /// The return value is either:
    /// - An empty string if self is equal to [`RoutingParameters::default`].
    /// - Or a bech32 string without the leading hrp and separator.
    pub(crate) fn encode(&self) -> String {
        let mut encoded = Vec::new();

        if let Some((tag_len, interface)) = self.receiver_profile {
            let interface = interface as u16;
            debug_assert_eq!(
                interface >> 11,
                0,
                "address interface should have its upper 5 bits unset"
            );

            // The interface takes up 11 bits and the tag length 5 bits, so we can merge them
            // together.
            let tag_len = (tag_len as u16) << 11;
            let receiver_profile: u16 = tag_len | interface;
            let receiver_profile: [u8; 2] = receiver_profile.to_be_bytes();

            // Append the receiver profile key and the encoded value to the vector.
            encoded.push(RECEIVER_PROFILE_KEY);
            encoded.extend(receiver_profile);
        }

        if encoded.is_empty() {
            return String::new();
        }

        let bech32_str =
            bech32::encode::<Bech32m>(*ROUTING_PARAMETERS_HRP, &encoded).expect("TODO");
        let encoded_str = bech32_str
            .strip_prefix(ROUTING_PARAMETERS_HRP.as_str())
            .expect("bech32 str should start with the hrp");
        let encoded_str = encoded_str
            .strip_prefix(BECH32_SEPARATOR)
            .expect("encoded str should start with bech32 separator `1`");
        encoded_str.to_owned()
    }

    /// Decodes [`RoutingParameters`] from a bech32 string _without_ the leading hrp and separator.
    ///
    /// The string must be either:
    /// - An empty string, in which case [`RoutingParameters::default`] is returned.
    /// - Or a validly encoded bech32 string without the leading hrp and separator.
    pub(crate) fn decode(mut bech32_string: String) -> Result<Self, AddressError> {
        if bech32_string.is_empty() {
            return Ok(RoutingParameters::new());
        }

        // ------ Decode bech32 string into bytes ------

        // Reinsert the expected HRP into the string that is stripped during encoding.
        bech32_string.insert_str(0, BECH32_SEPARATOR);
        bech32_string.insert_str(0, ROUTING_PARAMETERS_HRP.as_str());

        // We use CheckedHrpString with an explicit checksum algorithm so we don't allow the
        // `Bech32` or `NoChecksum` algorithms.
        let checked_string =
            CheckedHrpstring::new::<Bech32m>(&bech32_string).map_err(|source| {
                // The CheckedHrpStringError does not implement core::error::Error, only
                // std::error::Error, so for now we convert it to a String. Even if it will
                // implement the trait in the future, we should include it as an opaque
                // error since the crate does not have a stable release yet.
                AddressError::routing_parameters_decode_with_source(
                    "failed to decode routing parameters bech32 string",
                    Bech32Error::DecodeError(source.to_string().into()),
                )
            })?;

        // ------ Decode bytes into routing parameters ------

        let mut routing_parameters = RoutingParameters::new();
        let mut byte_iter = checked_string.byte_iter();

        while let Some(key) = byte_iter.next() {
            match key {
                RECEIVER_PROFILE_KEY => {
                    if byte_iter.len() < 2 {
                        return Err(AddressError::routing_parameters_decode(
                            "expected two bytes to decode receiver profile",
                        ));
                    };

                    let byte0 = byte_iter.next().expect("byte0 should exist");
                    let byte1 = byte_iter.next().expect("byte1 should exist");
                    let receiver_profile = u16::from_be_bytes([byte0, byte1]);

                    let tag_len = (receiver_profile >> 11) as u8;
                    let interface = receiver_profile & 0b0000_0111_1111_1111;
                    let interface = AddressInterface::try_from(interface).map_err(|err| {
                        AddressError::routing_parameters_decode_with_source(
                            "failed to decode address interface",
                            err,
                        )
                    })?;

                    routing_parameters =
                        routing_parameters.with_receiver_profile(tag_len, interface);
                },
                other => {
                    return Err(AddressError::UnknownRoutingParameterKey(other));
                },
            }
        }

        Ok(routing_parameters)
    }
}

#[cfg(test)]
mod tests {
    use bech32::{Bech32m, Checksum, Hrp};

    use super::*;

    /// Checks the assumptions about the total length allowed in bech32 encoding.
    ///
    /// The assumption is that encoding should error if the total length of the hrp + data (encoded
    /// in GF(32)) + the separator + the checksum exceeds Bech32m::CODE_LENGTH.
    #[test]
    fn bech32_code_length_assertions() -> anyhow::Result<()> {
        let hrp = Hrp::parse("mrp").unwrap();
        let separator_len = BECH32_SEPARATOR.len();
        // The fixed number of characters included in a bech32 string.
        let fixed_num_bytes = hrp.as_str().len() + separator_len + Bech32m::CHECKSUM_LENGTH;
        let num_allowed_chars = Bech32m::CODE_LENGTH - fixed_num_bytes;
        // Multiply by the 5 bits per base32 character and divide by 8 bits per byte.
        let num_allowed_bytes = num_allowed_chars * 5 / 8;

        // The number of bytes that routing parameters effectively have available.
        assert_eq!(num_allowed_bytes, 633);

        // This amount of data is the max that should be okay to encode.
        let data_ok = vec![5; num_allowed_bytes];
        // One more byte than the max allowed amount should result in an error.
        let data_too_long = vec![5; num_allowed_bytes + 1];

        assert!(bech32::encode::<Bech32m>(hrp, &data_ok).is_ok());
        assert!(bech32::encode::<Bech32m>(hrp, &data_too_long).is_err());

        Ok(())
    }

    #[test]
    fn routing_parameters_bech32_encode_decode_roundtrip() -> anyhow::Result<()> {
        let empty_routing_params = RoutingParameters::default();
        assert!(empty_routing_params.encode().is_empty());
        assert_eq!(RoutingParameters::decode(empty_routing_params.encode())?, empty_routing_params);

        let routing_params =
            RoutingParameters::new().with_receiver_profile(8, AddressInterface::BasicWallet);
        assert_eq!(routing_params, RoutingParameters::decode(routing_params.encode())?);

        Ok(())
    }
}
