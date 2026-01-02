use core::fmt;

use miden_crypto::Felt;

use super::{
    AccountId,
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    NoteError,
    Serializable,
};
use crate::account::AccountStorageMode;

// NOTE TAG
// ================================================================================================

/// [`NoteTag`]s are 32-bits of data that serve as best-effort filters for notes.
///
/// Tags enable quick lookups for notes related to particular use cases, scripts, or account
/// prefixes.
///
/// ## Account Targets
///
/// A note targeted at an account is a note that is intended or even enforced to be consumed by a
/// specific account. One example is a P2ID note that enforces that it can only be consumed by a
/// specific account ID. The tag for such a P2ID note should make it easy for the receiver to find
/// the note. Therefore, the tag encodes a certain number of bits of the receiver account's ID, by
/// convention. Notably, it may not encode the full 32 bits of the target account's ID to preserve
/// the receiver's privacy. See also the section on privacy below.
///
/// Because this convention is widely used, the note tag provides a dedicated constructor for this:
/// [`NoteTag::from_account_id`].
///
/// ## Use Cases
///
/// Use case notes are notes that are not intended to be consumed by a specific account, but by
/// anyone willing to fulfill the note's contract. One example is a SWAP note that trades one asset
/// against another. Such a use case note can define the structure of their note tags. A sensible
/// structure for a SWAP note could be:
/// - encoding the 2 bits of the note's type.
/// - encoding the note script root, i.e. making it identifiable as a SWAP note, for example by
///   using 16 bits of the SWAP script root.
/// - encoding the SWAP pair, for example by using 8 bits of the offered asset faucet ID and 8 bits
///   of the requested asset faucet ID.
///
/// This allows clients to search for a public SWAP note that trades USDC against ETH only through
/// the note tag. Since tags are not validated in any way and only act as best-effort filters,
/// further local filtering is almost always necessary. For example, there could easily be a
/// collision on the 8 bits used in SWAP tag's faucet IDs.
///
/// ## Privacy vs Efficiency
///
/// Using note tags strikes a balance between privacy and efficiency. Without tags, querying a
/// specific note ID reveals a user's interest to the node. Conversely, downloading and filtering
/// all registered notes locally is highly inefficient. Tags allow users to adjust their level of
/// privacy by choosing how broadly or narrowly they define their search criteria, letting them find
/// the right balance between revealing too much information and incurring excessive computational
/// overhead.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct NoteTag(u32);

impl NoteTag {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The default note tag length for an account ID with local execution.
    pub const DEFAULT_LOCAL_TAG_LENGTH: u8 = 14;
    /// The default note tag length for an account ID with network execution.
    pub const DEFAULT_NETWORK_TAG_LENGTH: u8 = 30;
    /// The maximum number of bits that can be encoded into the tag for local accounts.
    pub const MAX_LOCAL_TAG_LENGTH: u8 = 30;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NoteTag`] from an arbitrary `u32`.
    pub const fn new(tag: u32) -> Self {
        Self(tag)
    }

    /// Returns a note tag instantiated from the specified account ID.
    ///
    /// The tag is constructed as follows:
    ///
    /// - For local execution ([`AccountStorageMode::Private`] or [`AccountStorageMode::Public`]),
    ///   the two most significant bits are set to `0b00`. The following 14 bits are set to the most
    ///   significant bits of the account ID, and the remaining 16 bits are set to 0.
    /// - For network execution ([`AccountStorageMode::Network`]), the most significant bits are set
    ///   to `0b00` and the remaining bits are set to the 30 most significant bits of the account
    ///   ID.
    pub fn from_account_id(account_id: AccountId) -> Self {
        match account_id.storage_mode() {
            AccountStorageMode::Network => Self::from_network_account_id(account_id),
            AccountStorageMode::Private | AccountStorageMode::Public => {
                // safe to unwrap since DEFAULT_LOCAL_TAG_LENGTH < MAX_LOCAL_TAG_LENGTH
                Self::from_account_id_and_tag_len(account_id, Self::DEFAULT_LOCAL_TAG_LENGTH)
                    .unwrap()
            },
        }
    }

    /// Constructs a note tag from the given `account_id` and `tag_len`.
    ///
    /// The tag is constructed by:
    /// - Setting the two most significant bits to zero.
    /// - The next `tag_len` bits are set to the most significant bits of the account ID prefix.
    /// - The remaining bits are set to zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `tag_len` is larger than [`NoteTag::MAX_LOCAL_TAG_LENGTH`].
    pub fn from_account_id_and_tag_len(
        account_id: AccountId,
        tag_len: u8,
    ) -> Result<Self, NoteError> {
        if tag_len > Self::MAX_LOCAL_TAG_LENGTH {
            return Err(NoteError::NoteTagLengthTooLarge(tag_len));
        }

        let prefix_id: u64 = account_id.prefix().into();

        // Shift the high bits of the account ID such that they are laid out as:
        // [34 zero bits | remaining high bits (30 bits)].
        let high_bits = prefix_id >> 34;

        // This is equivalent to the following layout, interpreted as a u32:
        // [2 zero bits | remaining high bits (30 bits)].
        let high_bits = high_bits as u32;

        // Select the top `tag_len` bits of the account ID, i.e.:
        // [2 zero bits | remaining high bits (tag_len bits) | (30 - tag_len) zero bits].
        let high_bits = high_bits & (u32::MAX << (32 - 2 - tag_len));

        Ok(Self(high_bits))
    }

    /// Constructs a network account note tag from the specified `account_id`.
    ///
    /// The tag is constructed as follows:
    ///
    /// - The two most significant bits are set to `0b00`.
    /// - The remaining bits are set to the 30 most significant bits of the account ID.
    pub(crate) fn from_network_account_id(account_id: AccountId) -> Self {
        let prefix_id: u64 = account_id.prefix().into();

        // Shift the high bits of the account ID such that they are laid out as:
        // [34 zero bits | remaining high bits (30 bits)].
        let high_bits = prefix_id >> 34;

        // This is equivalent to the following layout, interpreted as a u32:
        // [2 zero bits | remaining high bits (30 bits)].
        Self(high_bits as u32)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the inner u32 value of this tag.
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for NoteTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u32())
    }
}

// CONVERSIONS INTO NOTE TAG
// ================================================================================================

impl From<u32> for NoteTag {
    fn from(tag: u32) -> Self {
        Self::new(tag)
    }
}

// CONVERSIONS FROM NOTE TAG
// ================================================================================================

impl From<NoteTag> for u32 {
    fn from(tag: NoteTag) -> Self {
        tag.as_u32()
    }
}

impl From<NoteTag> for Felt {
    fn from(tag: NoteTag) -> Self {
        Felt::from(tag.as_u32())
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteTag {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.as_u32().write_into(target);
    }
}

impl Deserializable for NoteTag {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tag = u32::read_from(source)?;
        Ok(Self::new(tag))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use super::NoteTag;
    use crate::account::AccountId;
    use crate::testing::account_id::{
        ACCOUNT_ID_NETWORK_FUNGIBLE_FAUCET,
        ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PRIVATE_SENDER,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3,
        ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1,
        ACCOUNT_ID_REGULAR_NETWORK_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE_ON_CHAIN_2,
        ACCOUNT_ID_SENDER,
    };

    #[test]
    fn from_account_id() {
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

        for account_id in private_accounts.iter().chain(public_accounts.iter()) {
            let tag = NoteTag::from_account_id(*account_id);
            assert_eq!(tag.as_u32() >> 30, 0, "two most significant bits should be zero");
            assert_eq!(tag.as_u32() << 16, 0, "16 least significant bits should be zero");
            assert_eq!(
                (account_id.prefix().as_u64() >> 50) as u32,
                tag.as_u32() >> 16,
                "14 most significant bits should match"
            );
        }

        for account_id in network_accounts {
            let tag = NoteTag::from_account_id(account_id);
            assert_eq!(tag.as_u32() >> 30, 0, "two most significant bits should be zero");
            assert_eq!(
                account_id.prefix().as_u64() >> 34,
                tag.as_u32() as u64,
                "30 most significant bits should match"
            );
        }
    }

    #[test]
    fn from_custom_account_target() -> anyhow::Result<()> {
        let account_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;
        let tag = NoteTag::from_account_id_and_tag_len(account_id, NoteTag::MAX_LOCAL_TAG_LENGTH)?;

        assert_eq!(tag.as_u32() >> 30, 0, "two most significant bits should be zero");
        assert_eq!(
            (account_id.prefix().as_u64() >> 34) as u32,
            tag.as_u32(),
            "30 most significant bits should match"
        );

        Ok(())
    }
}
