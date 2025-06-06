use core::{fmt, num::TryFromIntError};

use miden_crypto::Felt;

use super::{
    AccountId, ByteReader, ByteWriter, Deserializable, DeserializationError, NoteError, NoteType,
    Serializable,
};
use crate::account::AccountStorageMode;

// CONSTANTS
// ================================================================================================
const NETWORK_EXECUTION: u8 = 0;
const LOCAL_EXECUTION: u8 = 1;

// The 2 most significant bits are set to `0b11`.
const LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED: u32 = 0xc000_0000;
// The 2 most significant bits are set to `0b10`.
const PUBLIC_USECASE: u32 = 0x8000_0000;

/// [super::Note]'s execution mode hints.
///
/// The execution hints are _not_ enforced, therefore function only as hints. For example, if a
/// note's tag is created with the [NoteExecutionMode::Network], further validation is necessary to
/// check the account_id is known, that the account's state is on-chain, and the account is
/// controlled by the network.
///
/// The goal of the hint is to allow for a network node to quickly filter notes that are not
/// intended for network execution, and skip the validation steps mentioned above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NoteExecutionMode {
    Network = NETWORK_EXECUTION,
    Local = LOCAL_EXECUTION,
}

// NOTE TAG
// ================================================================================================

/// [NoteTag]`s are best effort filters for notes registered with the network.
///
/// Tags are light-weight values used to speed up queries. The 2 most significant bits of the tags
/// have the following interpretation:
///
/// | Prefix | Execution mode | Target   | Allowed [NoteType] |
/// | ------ | :------------: | :------: | :----------------: |
/// | `0b00` | Network        | Specific | [NoteType::Public] |
/// | `0b01` | Network        | Use case | [NoteType::Public] |
/// | `0b10` | Local          | Any      | [NoteType::Public] |
/// | `0b11` | Local          | Any      | Any                |
///
/// Where:
///
/// - [`NoteExecutionMode`] is set to [`NoteExecutionMode::Network`] to hint a [`Note`](super::Note)
///   should be consumed by the network. These notes will be further validated and if possible
///   consumed by it.
/// - Target describes how to further interpret the bits in the tag. For tags with a specific
///   target, the rest of the tag is interpreted as a partial [`AccountId`]. For use case values,
///   the meaning of the rest of the tag is not specified by the protocol and can be used by
///   applications built on top of the chain.
///
/// The note type is the only value enforced by the protocol. The rationale is that any note
/// intended to be consumed by the network must be public to have all the details available. The
/// public note for local execution is intended to allow users to search for notes that can be
/// consumed right away, without requiring an off-band communication channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NoteTag(u32);

impl NoteTag {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The exponent of the maximum allowed use case id. In other words, 2^exponent is the maximum
    /// allowed use case id.
    pub(crate) const MAX_USE_CASE_ID_EXPONENT: u8 = 14;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new [NoteTag] instantiated from the specified account ID.
    ///
    /// The tag is constructed as follows:
    ///
    /// - For local execution(`AccountStorageMode::Private` or `AccountStorageMode::Public`), the
    ///   two most significant bits are set to `0b11`, which allows for any note type to be used.
    ///   The following 14 bits are set to the most significant bits of the account ID, and the
    ///   remaining 16 bits are set to 0.
    /// - For network execution (`AccountStorageMode::Network`), the most significant bits are set
    ///   to `0b00` and the remaining bits are set to the 30 most significant bits of the account
    ///   ID.
    pub fn from_account_id(account_id: AccountId) -> Self {
        match account_id.storage_mode() {
            AccountStorageMode::Network => {
                let prefix_id: u64 = account_id.prefix().into();

                // Shift the high bits of the account ID such that they are layed out as:
                // [34 zero bits | remaining high bits (30 bits)].
                let high_bits = prefix_id >> 34;

                // This is equivalent to the following layout, interpreted as a u32:
                // [2 zero bits | remaining high bits (30 bits)].
                // The two most significant zero bits match the tag we need for network
                // execution.
                Self(high_bits as u32)
            },
            AccountStorageMode::Private | AccountStorageMode::Public => {
                let prefix_id: u64 = account_id.prefix().into();

                // Shift the high bits of the account ID such that they are layed out as:
                // [34 zero bits | remaining high bits (30 bits)].
                let high_bits = prefix_id >> 34;

                // This is equivalent to the following layout, interpreted as a u32:
                // [2 zero bits | remaining high bits (30 bits)].
                let high_bits = high_bits as u32;

                // Select the upper half of the u32 which then contains the 14 most significant bits
                // of the account ID, i.e.:
                // [2 zero bits | remaining high bits (14 bits) | 16 zero bits].
                let high_bits = high_bits & 0xffff0000;

                // Set the local execution tag in the two most significant bits.
                Self(high_bits | LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED)
            },
        }
    }

    /// Returns a new [NoteTag] instantiated for a custom use case which requires a public note.
    ///
    /// The public use_case tag requires a [NoteType::Public] note.
    ///
    /// The two high bits are set to the `b10` or `b01` depending on the execution hint, the next 14
    /// bits are set to the `use_case_id`, and the low 16 bits are set to `payload`.
    ///
    /// # Errors
    ///
    /// - If `use_case_id` is larger than or equal to $2^{14}$.
    pub fn for_public_use_case(
        use_case_id: u16,
        payload: u16,
        execution: NoteExecutionMode,
    ) -> Result<Self, NoteError> {
        if (use_case_id >> 14) != 0 {
            return Err(NoteError::NoteTagUseCaseTooLarge(use_case_id));
        }

        let execution_bits = match execution {
            NoteExecutionMode::Local => PUBLIC_USECASE, // high bits set to `0b10`
            NoteExecutionMode::Network => 0x40000000,   // high bits set to `0b01`
        };

        let use_case_bits = (use_case_id as u32) << 16;
        let payload_bits = payload as u32;

        Ok(Self(execution_bits | use_case_bits | payload_bits))
    }

    /// Returns a new [NoteTag] instantiated for a custom local use case.
    ///
    /// The local use_case tag is the only tag type that allows for [NoteType::Private] notes.
    ///
    /// The two high bits are set to the `b11`, the next 14 bits are set to the `use_case_id`, and
    /// the low 16 bits are set to `payload`.
    ///
    /// # Errors
    ///
    /// - If `use_case_id` is larger than or equal to 2^14.
    pub fn for_local_use_case(use_case_id: u16, payload: u16) -> Result<Self, NoteError> {
        if (use_case_id >> NoteTag::MAX_USE_CASE_ID_EXPONENT) != 0 {
            return Err(NoteError::NoteTagUseCaseTooLarge(use_case_id));
        }

        let execution_bits = LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED;
        let use_case_bits = (use_case_id as u32) << 16;
        let payload_bits = payload as u32;

        Ok(Self(execution_bits | use_case_bits | payload_bits))
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns true if the note is intended for execution by a specific account.
    ///
    /// A note is intended for execution by a single account if the first two bits are zeros
    pub fn is_single_target(&self) -> bool {
        let first_2_bit = self.0 >> 30;
        first_2_bit == 0b00
    }

    /// Returns note execution mode defined by this tag.
    ///
    /// If the most significant bit of the tag is 0 the note is intended for local execution;
    /// otherwise, the note is intended for network execution.
    pub fn execution_mode(&self) -> NoteExecutionMode {
        let first_bit = self.0 >> 31;

        if first_bit == (LOCAL_EXECUTION as u32) {
            NoteExecutionMode::Local
        } else {
            NoteExecutionMode::Network
        }
    }

    /// Returns the inner u32 value of this tag.
    pub fn inner(&self) -> u32 {
        self.0
    }

    // UTILITY METHODS
    // --------------------------------------------------------------------------------------------

    /// Returns an error if this tag is not consistent with the specified note type, and self
    /// otherwise.
    pub fn validate(&self, note_type: NoteType) -> Result<Self, NoteError> {
        if self.execution_mode() == NoteExecutionMode::Network && note_type != NoteType::Public {
            return Err(NoteError::NetworkExecutionRequiresPublicNote(note_type));
        }

        let is_public_use_case = (self.0 & 0xc0000000) == PUBLIC_USECASE;
        if is_public_use_case && note_type != NoteType::Public {
            Err(NoteError::PublicUseCaseRequiresPublicNote(note_type))
        } else {
            Ok(*self)
        }
    }
}

impl fmt::Display for NoteTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// CONVERSIONS INTO NOTE TAG
// ================================================================================================

impl From<u32> for NoteTag {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl TryFrom<u64> for NoteTag {
    type Error = TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Ok(Self(value.try_into()?))
    }
}

impl TryFrom<Felt> for NoteTag {
    type Error = TryFromIntError;

    fn try_from(value: Felt) -> Result<Self, Self::Error> {
        Ok(Self(value.as_int().try_into()?))
    }
}

// CONVERSIONS FROM NOTE TAG
// ================================================================================================

impl From<NoteTag> for u32 {
    fn from(value: NoteTag) -> Self {
        value.0
    }
}

impl From<NoteTag> for u64 {
    fn from(value: NoteTag) -> Self {
        value.0 as u64
    }
}

impl From<NoteTag> for Felt {
    fn from(value: NoteTag) -> Self {
        Felt::from(value.0)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteTag {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.0.write_into(target);
    }
}

impl Deserializable for NoteTag {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tag = u32::read_from(source)?;
        Ok(Self(tag))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::{NoteExecutionMode, NoteTag};
    use crate::{
        NoteError,
        account::AccountId,
        note::NoteType,
        testing::account_id::{
            ACCOUNT_ID_NETWORK_FUNGIBLE_FAUCET, ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET, ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PRIVATE_SENDER, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3, ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1,
            ACCOUNT_ID_REGULAR_NETWORK_ACCOUNT_IMMUTABLE_CODE,
            ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE_ON_CHAIN_2, ACCOUNT_ID_SENDER,
        },
    };

    #[test]
    fn test_from_account_id() {
        let private_accounts = [
            AccountId::try_from(ACCOUNT_ID_SENDER).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap(),
            AccountId::try_from(ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET).unwrap(),
        ];
        let public_accounts = [
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap(),
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2).unwrap(),
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE).unwrap(),
            AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE_ON_CHAIN_2)
                .unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET).unwrap(),
            AccountId::try_from(ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1).unwrap(),
        ];
        let network_accounts = [
            AccountId::try_from(ACCOUNT_ID_REGULAR_NETWORK_ACCOUNT_IMMUTABLE_CODE).unwrap(),
            AccountId::try_from(ACCOUNT_ID_NETWORK_FUNGIBLE_FAUCET).unwrap(),
            AccountId::try_from(ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET).unwrap(),
        ];

        for account_id in network_accounts {
            let tag = NoteTag::from_account_id(account_id);
            assert!(tag.is_single_target());
            assert_eq!(tag.execution_mode(), NoteExecutionMode::Network);

            tag.validate(NoteType::Public)
                .expect("network execution should require notes to be public");
            assert_matches!(
                tag.validate(NoteType::Private),
                Err(NoteError::NetworkExecutionRequiresPublicNote(NoteType::Private))
            );
            assert_matches!(
                tag.validate(NoteType::Encrypted),
                Err(NoteError::NetworkExecutionRequiresPublicNote(NoteType::Encrypted))
            );
        }

        for account_id in private_accounts {
            let tag = NoteTag::from_account_id(account_id);
            assert!(!tag.is_single_target());
            assert_eq!(tag.execution_mode(), NoteExecutionMode::Local);

            tag.validate(NoteType::Public)
                .expect("local execution should support public notes");
            tag.validate(NoteType::Private)
                .expect("local execution should support private notes");
            tag.validate(NoteType::Encrypted)
                .expect("local execution should support encrypted notes");
        }

        for account_id in public_accounts {
            let tag = NoteTag::from_account_id(account_id);
            assert!(!tag.is_single_target());
            assert_eq!(tag.execution_mode(), NoteExecutionMode::Local);

            tag.validate(NoteType::Public)
                .expect("local execution should support public notes");
            tag.validate(NoteType::Private)
                .expect("local execution should support private notes");
            tag.validate(NoteType::Encrypted)
                .expect("local execution should support encrypted notes");
        }

        for account_id in network_accounts {
            let tag = NoteTag::from_account_id(account_id);
            assert!(tag.is_single_target());
            assert_eq!(tag.execution_mode(), NoteExecutionMode::Network);

            tag.validate(NoteType::Public)
                .expect("local execution should support public notes");
            assert_matches!(
                tag.validate(NoteType::Private),
                Err(NoteError::NetworkExecutionRequiresPublicNote(NoteType::Private))
            );
            assert_matches!(
                tag.validate(NoteType::Encrypted),
                Err(NoteError::NetworkExecutionRequiresPublicNote(NoteType::Encrypted))
            );
        }
    }

    #[test]
    fn test_from_account_id_values() {
        /// Private Account ID with the following bit pattern in the first and second byte:
        /// 0b11001100_01010101
        ///   ^^^^^^^^ ^^^^^^  <- 14 bits of the local tag.
        const PRIVATE_ACCOUNT_INT: u128 = ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE
            | 0x0055_0000_0000_0000_0000_0000_0000_0000;
        let private_account_id = AccountId::try_from(PRIVATE_ACCOUNT_INT).unwrap();

        // Expected private tag with LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED.
        let expected_private_local_tag = NoteTag(0b11110011_00010101_00000000_00000000);

        /// Public Account ID with the following bit pattern in the first and second byte:
        /// 0b10101010_01010101_11001100_10101010
        ///   ^^^^^^^^ ^^^^^^  <- 14 bits of the local tag.
        const PUBLIC_ACCOUNT_INT: u128 = ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE
            | 0x0055_ccaa_0000_0000_0000_0000_0000_0000;
        let public_account_id = AccountId::try_from(PUBLIC_ACCOUNT_INT).unwrap();

        // Expected public tag with LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED.
        let expected_public_local_tag = NoteTag(0b11101010_10010101_00000000_00000000);

        /// Network Account ID with the following bit pattern in the first and second byte:
        /// 0b10101010_11001100_01110111_11001100
        ///   ^^^^^^^^ ^^^^^^^^ ^^^^^^^^ ^^^^^^  <- 30 bits of the network tag.
        ///   ^^^^^^^^ ^^^^^^  <- 14 bits of the local tag.
        const NETWORK_ACCOUNT_INT: u128 = ACCOUNT_ID_REGULAR_NETWORK_ACCOUNT_IMMUTABLE_CODE
            | 0x00cc_77cc_0000_0000_0000_0000_0000_0000;
        let network_account_id = AccountId::try_from(NETWORK_ACCOUNT_INT).unwrap();

        // Expected network tag with LOCAL_EXECUTION_WITH_ALL_NOTE_TYPES_ALLOWED.
        let _expected_network_local_tag = NoteTag(0b11101010_10110011_00000000_00000000);

        // Expected network tag with leading 00 tag bits for network execution.
        let expected_network_network_tag = NoteTag(0b00101010_10110011_00011101_11110011);

        assert_eq!(NoteTag::from_account_id(private_account_id), expected_private_local_tag,);
        assert_eq!(NoteTag::from_account_id(public_account_id), expected_public_local_tag,);

        assert_eq!(NoteTag::from_account_id(network_account_id), expected_network_network_tag,);
    }

    #[test]
    fn test_for_public_use_case() {
        // NETWORK
        // ----------------------------------------------------------------------------------------
        let tag = NoteTag::for_public_use_case(0b0, 0b0, NoteExecutionMode::Network).unwrap();
        assert_eq!(tag, NoteTag(0b01000000_00000000_00000000_00000000));

        tag.validate(NoteType::Public).unwrap();

        assert_matches!(
            tag.validate(NoteType::Private).unwrap_err(),
            NoteError::NetworkExecutionRequiresPublicNote(NoteType::Private)
        );
        assert_matches!(
            tag.validate(NoteType::Encrypted).unwrap_err(),
            NoteError::NetworkExecutionRequiresPublicNote(NoteType::Encrypted)
        );

        let tag = NoteTag::for_public_use_case(0b1, 0b0, NoteExecutionMode::Network).unwrap();
        assert_eq!(tag, NoteTag(0b01000000_00000001_00000000_00000000));

        let tag = NoteTag::for_public_use_case(0b0, 0b1, NoteExecutionMode::Network).unwrap();
        assert_eq!(tag, NoteTag(0b01000000_00000000_00000000_00000001));

        let tag = NoteTag::for_public_use_case(1 << 13, 0b0, NoteExecutionMode::Network).unwrap();
        assert_eq!(tag, NoteTag(0b01100000_00000000_00000000_00000000));

        // LOCAL
        // ----------------------------------------------------------------------------------------
        let tag = NoteTag::for_public_use_case(0b0, 0b0, NoteExecutionMode::Local).unwrap();
        assert_eq!(tag, NoteTag(0b10000000_00000000_00000000_00000000));

        tag.validate(NoteType::Public).unwrap();
        assert_matches!(
            tag.validate(NoteType::Private).unwrap_err(),
            NoteError::PublicUseCaseRequiresPublicNote(NoteType::Private)
        );
        assert_matches!(
            tag.validate(NoteType::Encrypted).unwrap_err(),
            NoteError::PublicUseCaseRequiresPublicNote(NoteType::Encrypted)
        );

        let tag = NoteTag::for_public_use_case(0b0, 0b1, NoteExecutionMode::Local).unwrap();
        assert_eq!(tag, NoteTag(0b10000000_00000000_00000000_00000001));

        let tag = NoteTag::for_public_use_case(0b1, 0b0, NoteExecutionMode::Local).unwrap();
        assert_eq!(tag, NoteTag(0b10000000_00000001_00000000_00000000));

        let tag = NoteTag::for_public_use_case(1 << 13, 0b0, NoteExecutionMode::Local).unwrap();
        assert_eq!(tag, NoteTag(0b10100000_00000000_00000000_00000000));

        assert_matches!(
          NoteTag::for_public_use_case(1 << 15, 0b0, NoteExecutionMode::Local).unwrap_err(),
          NoteError::NoteTagUseCaseTooLarge(use_case) if use_case == 1 << 15
        );
        assert_matches!(
          NoteTag::for_public_use_case(1 << 14, 0b0, NoteExecutionMode::Local).unwrap_err(),
          NoteError::NoteTagUseCaseTooLarge(use_case) if use_case == 1 << 14
        );
    }

    #[test]
    fn test_for_private_use_case() {
        let tag = NoteTag::for_local_use_case(0b0, 0b0).unwrap();
        assert_eq!(tag, NoteTag(0b11000000_00000000_00000000_00000000));

        tag.validate(NoteType::Public)
            .expect("local execution should support public notes");
        tag.validate(NoteType::Private)
            .expect("local execution should support private notes");
        tag.validate(NoteType::Encrypted)
            .expect("local execution should support encrypted notes");

        let tag = NoteTag::for_local_use_case(0b0, 0b1).unwrap();
        assert_eq!(tag, NoteTag(0b11000000_00000000_00000000_00000001));

        let tag = NoteTag::for_local_use_case(0b1, 0b0).unwrap();
        assert_eq!(tag, NoteTag(0b11000000_00000001_00000000_00000000));

        let tag = NoteTag::for_local_use_case(1 << 13, 0b0).unwrap();
        assert_eq!(tag, NoteTag(0b11100000_00000000_00000000_00000000));

        assert_matches!(
          NoteTag::for_local_use_case(1 << 15, 0b0).unwrap_err(),
          NoteError::NoteTagUseCaseTooLarge(use_case) if use_case == 1 << 15
        );
        assert_matches!(
          NoteTag::for_local_use_case(1 << 14, 0b0).unwrap_err(),
          NoteError::NoteTagUseCaseTooLarge(use_case) if use_case == 1 << 14
        );
    }
}
