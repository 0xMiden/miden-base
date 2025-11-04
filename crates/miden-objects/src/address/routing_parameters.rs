use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use bech32::primitives::decode::CheckedHrpstring;
use bech32::{Bech32m, Hrp};

use crate::AddressError;
use crate::address::AddressInterface;
use crate::crypto::PublicEncryptionKey;
use crate::errors::Bech32Error;
use crate::note::NoteTag;
use crate::utils::serde::{
    ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable,
};
use crate::utils::sync::LazyLock;

/// The HRP used for encoding routing parameters.
///
/// This HRP is only used internally, but needs to be well-defined for other routing parameter
/// encode/decode implementations.
///
/// `mrp` stands for Miden Routing Parameters.
static ROUTING_PARAMETERS_HRP: LazyLock<Hrp> =
    LazyLock::new(|| Hrp::parse("mrp").expect("hrp should be valid"));

/// The separator character used in bech32.
const BECH32_SEPARATOR: &str = "1";

/// The value to encode the absence of a note tag routing parameter (i.e. `None`).
///
/// Note tag length is ensured to be <= [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and so 1 << 5 = 32 is used
/// to encode `None`.
const ABSENT_NOTE_TAG_LEN: u8 = 1 << 5;

/// The routing parameter key for the receiver profile.
const RECEIVER_PROFILE_KEY: u8 = 0;

/// The routing parameter key for the recipient's public encryption key.
const ENCRYPTION_KEY_KEY: u8 = 1;

/// The number of bytes expected in the encoded public encryption key.
const ENCRYPTION_KEY_NUM_BYTES: usize = 32;

/// Parameters that define how a sender should route a note to the [`AddressId`](super::AddressId)
/// in an [`Address`](super::Address).
///
/// Routing parameters capture three pieces of information:
/// - The [`AddressInterface`] exposed by the receiving account.
/// - An optional preferred note tag length.
/// - An optional public encryption key used to seal payloads for the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingParameters {
    interface: AddressInterface,
    note_tag_len: Option<u8>,
    encryption_key: Option<PublicEncryptionKey>,
}

impl RoutingParameters {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates new [`RoutingParameters`] from an [`AddressInterface`] and all other parameters
    /// initialized to `None`.
    pub fn new(interface: AddressInterface) -> Self {
        Self {
            interface,
            note_tag_len: None,
            encryption_key: None,
        }
    }

    /// Sets the note tag length routing parameter.
    ///
    /// The tag length determines how many bits of the address ID are encoded into [`NoteTag`]s of
    /// notes targeted to this address. This lets the receiver choose their level of privacy. A
    /// higher tag length makes the address ID more uniquely identifiable and reduces privacy,
    /// while a shorter length increases privacy at the cost of matching more notes
    /// published onchain.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tag length exceeds the maximum of [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and
    ///   [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`].
    pub fn with_note_tag_len(mut self, note_tag_len: u8) -> Result<Self, AddressError> {
        if note_tag_len > NoteTag::MAX_LOCAL_TAG_LENGTH {
            return Err(AddressError::TagLengthTooLarge(note_tag_len));
        }

        self.note_tag_len = Some(note_tag_len);
        Ok(self)
    }

    /// Sets the public encryption key routing parameter.
    ///
    /// The key must be a 32-byte X25519 public key (see [`PublicEncryptionKey`]).
    pub fn with_encryption_key(
        mut self,
        encryption_key: PublicEncryptionKey,
    ) -> Result<Self, AddressError> {
        validate_encryption_key(&encryption_key)?;
        self.encryption_key = Some(encryption_key);
        Ok(self)
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the note tag length preference.
    ///
    /// This is guaranteed to be in range `0..=30` (e.g. the maximum of
    /// [`NoteTag::MAX_LOCAL_TAG_LENGTH`] and [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`]).
    pub fn note_tag_len(&self) -> Option<u8> {
        self.note_tag_len
    }

    /// Returns the [`AddressInterface`] of the account to which the address points.
    pub fn interface(&self) -> AddressInterface {
        self.interface
    }

    /// Returns the public encryption key routing parameter, if present.
    ///
    /// The returned reference points to a 32-byte X25519 public key.
    pub fn encryption_key(&self) -> Option<&PublicEncryptionKey> {
        self.encryption_key.as_ref()
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Encodes [`RoutingParameters`] to a byte vector.
    pub(crate) fn encode_to_bytes(&self) -> Vec<u8> {
        let mut encoded = Vec::new();

        let note_tag_len = self.note_tag_len.unwrap_or(ABSENT_NOTE_TAG_LEN);

        let interface = self.interface as u16;
        debug_assert_eq!(
            interface >> 11,
            0,
            "address interface should have its upper 5 bits unset"
        );

        // The interface takes up 11 bits and the tag length 5 bits, so we can merge them
        // together.
        let tag_len = (note_tag_len as u16) << 11;
        let receiver_profile: u16 = tag_len | interface;
        let receiver_profile: [u8; 2] = receiver_profile.to_be_bytes();

        // Append the receiver profile key and the encoded value to the vector.
        encoded.push(RECEIVER_PROFILE_KEY);
        encoded.extend(receiver_profile);

        if let Some(encryption_key) = &self.encryption_key {
            encoded.push(ENCRYPTION_KEY_KEY);
            encoded.extend_from_slice(&encryption_key_to_bytes(encryption_key));
        }

        encoded
    }

    /// Encodes [`RoutingParameters`] to a bech32 string _without_ the leading hrp and separator.
    pub(crate) fn encode_to_string(&self) -> String {
        let encoded = self.encode_to_bytes();

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
    pub(crate) fn decode(mut bech32_string: String) -> Result<Self, AddressError> {
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
                AddressError::decode_error_with_source(
                    "failed to decode routing parameters bech32 string",
                    Bech32Error::DecodeError(source.to_string().into()),
                )
            })?;

        Self::decode_from_bytes(checked_string.byte_iter())
    }

    /// Decodes [`RoutingParameters`] from a byte iterator.
    pub(crate) fn decode_from_bytes(
        mut byte_iter: impl ExactSizeIterator<Item = u8>,
    ) -> Result<Self, AddressError> {
        let mut interface = None;
        let mut note_tag_len = None;
        let mut encryption_key = None;

        while let Some(key) = byte_iter.next() {
            match key {
                RECEIVER_PROFILE_KEY => {
                    if byte_iter.len() < 2 {
                        return Err(AddressError::decode_error(
                            "expected two bytes to decode receiver profile",
                        ));
                    };

                    let byte0 = byte_iter.next().expect("byte0 should exist");
                    let byte1 = byte_iter.next().expect("byte1 should exist");
                    let receiver_profile = u16::from_be_bytes([byte0, byte1]);

                    let tag_len = (receiver_profile >> 11) as u8;
                    note_tag_len = if tag_len == ABSENT_NOTE_TAG_LEN {
                        None
                    } else {
                        Some(tag_len)
                    };

                    let addr_interface = receiver_profile & 0b0000_0111_1111_1111;
                    let addr_interface =
                        AddressInterface::try_from(addr_interface).map_err(|err| {
                            AddressError::decode_error_with_source(
                                "failed to decode address interface",
                                err,
                            )
                        })?;
                    interface = Some(addr_interface);
                },
                ENCRYPTION_KEY_KEY => {
                    let remaining = byte_iter.len();
                    if remaining < ENCRYPTION_KEY_NUM_BYTES {
                        return Err(AddressError::InvalidEncryptionKeyLength {
                            expected: ENCRYPTION_KEY_NUM_BYTES,
                            actual: remaining,
                        });
                    }

                    if encryption_key.is_some() {
                        return Err(AddressError::DuplicateEncryptionKey);
                    }

                    let mut key_bytes = [0u8; ENCRYPTION_KEY_NUM_BYTES];
                    for byte in &mut key_bytes {
                        *byte = byte_iter.next().expect("encryption key byte should exist");
                    }

                    encryption_key = Some(encryption_key_from_bytes(&key_bytes)?);
                },
                other => {
                    return Err(AddressError::UnknownRoutingParameterKey(other));
                },
            }
        }

        let interface = interface.ok_or_else(|| {
            AddressError::decode_error("interface must be present in routing parameters")
        })?;

        let mut routing_parameters = RoutingParameters::new(interface);
        routing_parameters.note_tag_len = note_tag_len;
        routing_parameters.encryption_key = encryption_key;

        Ok(routing_parameters)
    }
}

impl Serializable for RoutingParameters {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let bytes = self.encode_to_bytes();
        // Due to the bech32 constraint of max 633 bytes, a u16 is sufficient.
        let num_bytes = bytes.len() as u16;

        target.write_u16(num_bytes);
        target.write_many(bytes);
    }
}

impl Deserializable for RoutingParameters {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_bytes = source.read_u16()?;
        let bytes: Vec<u8> = source.read_many(num_bytes as usize)?;

        Self::decode_from_bytes(bytes.into_iter())
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

fn validate_encryption_key(key: &PublicEncryptionKey) -> Result<(), AddressError> {
    match key {
        PublicEncryptionKey::X25519XChaCha20Poly1305(_) => Ok(()),
        _ => Err(AddressError::UnsupportedEncryptionKeyScheme),
    }
}

fn encryption_key_to_bytes(key: &PublicEncryptionKey) -> [u8; ENCRYPTION_KEY_NUM_BYTES] {
    match key {
        PublicEncryptionKey::X25519XChaCha20Poly1305(public_key) => {
            let bytes = public_key.to_bytes();
            let mut array = [0u8; ENCRYPTION_KEY_NUM_BYTES];
            array.copy_from_slice(&bytes);
            array
        },
        _ => unreachable!("unsupported encryption key variant stored in routing parameters"),
    }
}

fn encryption_key_from_bytes(
    bytes: &[u8; ENCRYPTION_KEY_NUM_BYTES],
) -> Result<PublicEncryptionKey, AddressError> {
    let public_key =
        miden_crypto::dsa::eddsa_25519::PublicKey::read_from_bytes(bytes).map_err(|err| {
            AddressError::decode_error_with_source("failed to decode encryption key", err)
        })?;

    Ok(PublicEncryptionKey::X25519XChaCha20Poly1305(public_key))
}

impl Hash for RoutingParameters {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.interface.hash(state);
        self.note_tag_len.hash(state);
        match &self.encryption_key {
            Some(key) => {
                state.write_u8(1);
                state.write(&encryption_key_to_bytes(key));
            },
            None => state.write_u8(0),
        }
    }
}

#[cfg(feature = "std")]
mod serde_impl {
    use super::*;
    use crate::utils::{bytes_to_hex_string, hex_to_bytes};
    use serde::de::Error as DeError;
    use serde::ser::SerializeStruct;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for RoutingParameters {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut state = serializer.serialize_struct("RoutingParameters", 3)?;
            state.serialize_field("interface", &self.interface)?;
            if let Some(note_tag_len) = self.note_tag_len {
                state.serialize_field("note_tag_len", &note_tag_len)?;
            }
            if let Some(encryption_key) = self.encryption_key.as_ref() {
                let encoded = bytes_to_hex_string(encryption_key_to_bytes(encryption_key));
                state.serialize_field("encryption_key", &encoded)?;
            }
            state.end()
        }
    }

    impl<'de> Deserialize<'de> for RoutingParameters {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct RoutingParametersSerde {
                interface: AddressInterface,
                #[serde(default)]
                note_tag_len: Option<u8>,
                #[serde(default)]
                encryption_key: Option<String>,
            }

            let helper = RoutingParametersSerde::deserialize(deserializer)?;

            let mut params = RoutingParameters::new(helper.interface);

            if let Some(note_tag_len) = helper.note_tag_len {
                params = params
                    .with_note_tag_len(note_tag_len)
                    .map_err(|err| DeError::custom(err.to_string()))?;
            }

            if let Some(encryption_key_hex) = helper.encryption_key {
                let key_bytes = hex_to_bytes::<ENCRYPTION_KEY_NUM_BYTES>(&encryption_key_hex)
                    .map_err(|err| DeError::custom(err.to_string()))?;
                let encryption_key = encryption_key_from_bytes(&key_bytes)
                    .map_err(|err| DeError::custom(err.to_string()))?;
                params = params
                    .with_encryption_key(encryption_key)
                    .map_err(|err| DeError::custom(err.to_string()))?;
            }

            Ok(params)
        }
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use bech32::{Bech32m, Checksum, Hrp};

    use super::*;
    use crate::AddressError;
    use miden_crypto::dsa::eddsa_25519::SecretKey as Ed25519SecretKey;

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
        let routing_params =
            RoutingParameters::new(AddressInterface::BasicWallet).with_note_tag_len(8)?;
        assert_eq!(routing_params, RoutingParameters::decode(routing_params.encode_to_string())?);

        Ok(())
    }

    /// Tests that routing parameters can be serialized and deserialized.
    #[test]
    fn routing_parameters_serialization() -> anyhow::Result<()> {
        let routing_params =
            RoutingParameters::new(AddressInterface::BasicWallet).with_note_tag_len(6)?;

        assert_eq!(
            routing_params,
            RoutingParameters::read_from_bytes(&routing_params.to_bytes()).unwrap()
        );

        Ok(())
    }

    #[test]
    fn routing_parameters_encryption_key_roundtrip() -> anyhow::Result<()> {
        let encryption_key = sample_encryption_key();
        let routing_params = RoutingParameters::new(AddressInterface::BasicWallet)
            .with_note_tag_len(10)?
            .with_encryption_key(encryption_key.clone())?;

        let decoded = RoutingParameters::decode(routing_params.encode_to_string())?;
        assert_eq!(decoded.encryption_key(), Some(&encryption_key));
        assert_eq!(routing_params, decoded);

        Ok(())
    }

    #[test]
    fn routing_parameters_rejects_short_encryption_key() {
        let mut encoded = RoutingParameters::new(AddressInterface::BasicWallet).encode_to_bytes();
        encoded.push(ENCRYPTION_KEY_KEY);
        encoded.push(1);

        let err = RoutingParameters::decode_from_bytes(encoded.into_iter()).unwrap_err();
        assert!(matches!(
            err,
            AddressError::InvalidEncryptionKeyLength { expected, actual }
                if expected == ENCRYPTION_KEY_NUM_BYTES && actual == 1
        ));
    }

    #[test]
    fn routing_parameters_rejects_duplicate_encryption_keys() {
        let mut encoded = RoutingParameters::new(AddressInterface::BasicWallet).encode_to_bytes();
        let key_bytes = [9u8; ENCRYPTION_KEY_NUM_BYTES];
        encoded.push(ENCRYPTION_KEY_KEY);
        encoded.extend_from_slice(&key_bytes);
        encoded.push(ENCRYPTION_KEY_KEY);
        encoded.extend_from_slice(&key_bytes);

        let err = RoutingParameters::decode_from_bytes(encoded.into_iter()).unwrap_err();
        assert!(matches!(err, AddressError::DuplicateEncryptionKey));
    }

    #[cfg(feature = "std")]
    #[test]
    fn routing_parameters_serde_roundtrip() -> anyhow::Result<()> {
        let encryption_key = sample_encryption_key();
        let routing_params = RoutingParameters::new(AddressInterface::BasicWallet)
            .with_note_tag_len(12)?
            .with_encryption_key(encryption_key.clone())?;

        let json = serde_json::to_string(&routing_params)?;
        let decoded: RoutingParameters = serde_json::from_str(&json)?;

        assert_eq!(decoded, routing_params);

        Ok(())
    }

    fn sample_encryption_key() -> PublicEncryptionKey {
        let mut rng = rand::rng();
        let secret_key = Ed25519SecretKey::with_rng(&mut rng);
        PublicEncryptionKey::X25519XChaCha20Poly1305(secret_key.public_key())
    }
}
