use core::fmt;

use miden_crypto::Felt;

use super::{
    AccountId,
    AccountStorageMode,
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    NoteError,
    Serializable,
};
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
/// specific account. One example is a P2ID note that can only be consumed by a specific account ID.
/// The tag for such a note should make it easy for the receiver to find the note. Therefore, the
/// tag encodes a certain number of bits of the receiver account's ID, by convention. Notably, it
/// may not encode the full 32 bits of the target account's ID to preserve the receiver's privacy.
/// See also the section on privacy below.
///
/// Because this convention is widely used, the note tag provides a dedicated constructor for this:
/// [`NoteTag::with_account_target`].
///
/// ## Use Case Tags
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
    pub const DEFAULT_LOCAL_ACCOUNT_TARGET_TAG_LENGTH: u8 = 14;
    /// The default note tag length for an account ID with network execution.
    pub const DEFAULT_NETWORK_ACCOUNT_TARGET_TAG_LENGTH: u8 = 32;
    /// The maximum number of bits that can be encoded into the tag for local accounts.
    pub const MAX_ACCOUNT_TARGET_TAG_LENGTH: u8 = 32;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NoteTag`] from an arbitrary `u32`.
    pub const fn new(tag: u32) -> Self {
        Self(tag)
    }

    /// Constructs a note tag that targets the given `account_id`.
    ///
    /// The tag is constructed as follows:
    ///
    /// - For local execution ([`AccountStorageMode::Private`] or [`AccountStorageMode::Public`]),
    ///   the two most significant bits are set to `0b00`. The following 14 bits are set to the most
    ///   significant bits of the account ID, and the remaining 16 bits are set to 0.
    /// - For network execution ([`AccountStorageMode::Network`]), the most significant bits are set
    ///   to `0b00` and the remaining bits are set to the 30 most significant bits of the account
    ///   ID.
    pub fn with_account_target(account_id: AccountId) -> Self {
        match account_id.storage_mode() {
            AccountStorageMode::Network => {
                let prefix = account_id.prefix().as_u64();
                Self((prefix >> 34) as u32)
            },
            _ => Self::with_custom_account_target(
                account_id,
                Self::DEFAULT_LOCAL_ACCOUNT_TARGET_TAG_LENGTH,
            )
            .unwrap(),
        }
    }

    /// Constructs a note tag that targets the given `account_id` with a custom `tag_len`.
    ///
    /// The tag is constructed by:
    /// - Setting the two most significant bits to zero.
    /// - The next `tag_len` bits are set to the most significant bits of the account ID prefix.
    /// - The remaining bits are set to zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `tag_len` is larger than [`NoteTag::MAX_ACCOUNT_TARGET_TAG_LENGTH`].
    pub fn with_custom_account_target(
        account_id: AccountId,
        tag_len: u8,
    ) -> Result<Self, NoteError> {
        if tag_len > Self::MAX_ACCOUNT_TARGET_TAG_LENGTH {
            return Err(NoteError::NoteTagLengthTooLarge(tag_len));
        }

        let prefix = account_id.prefix().as_u64();
        let extracted = (prefix >> (64 - tag_len)) as u32;

        let tag = if tag_len == 32 {
            extracted
        } else {
            extracted << (32 - tag_len)
        };

        Ok(Self(tag))
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

        for account_id in private_accounts.iter().chain(public_accounts.iter()) {
            let tag = NoteTag::with_account_target(*account_id);
            let tag_u32 = tag.as_u32();

            let used_bits = 32 - tag_u32.leading_zeros();

            if used_bits == 0 {
                assert_eq!(tag_u32, 0, "tag should be zero when no bits are used");
                continue;
            }

            let expected = (account_id.prefix().as_u64() >> (64 - used_bits)) as u32;
            let actual = tag_u32 >> (32 - used_bits);

            assert_eq!(actual, expected, "top {used_bits} bits should match account prefix");
        }

        let network_accounts = [
            AccountId::try_from(ACCOUNT_ID_REGULAR_NETWORK_ACCOUNT_IMMUTABLE_CODE).unwrap(),
            AccountId::try_from(ACCOUNT_ID_NETWORK_FUNGIBLE_FAUCET).unwrap(),
            AccountId::try_from(ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET).unwrap(),
        ];

        for account_id in network_accounts {
            let tag = NoteTag::with_account_target(account_id);

            assert_eq!(
                tag.as_u32() as u64,
                account_id.prefix().as_u64() >> 34,
                "network account tag must match prefix >> 34"
            );
        }
    }

    #[test]
    fn from_custom_account_target() -> anyhow::Result<()> {
        let account_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;
        let len = 32;

        let tag = NoteTag::with_custom_account_target(account_id, len)?;

        let prefix = account_id.prefix().as_u64();
        let expected = (prefix >> (64 - len)) as u32;

        assert_eq!(tag.as_u32(), expected, "full 32-bit tag should match account prefix");

        Ok(())
    }
}
