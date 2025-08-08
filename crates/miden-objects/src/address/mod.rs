use crate::AddressError;
use crate::account::{AccountId, AccountStorageMode};
use crate::note::{DEFAULT_LOCAL_TAG_LENGTH, MAX_LOCAL_TAG_LENGTH};

/// A user-facing address in Miden.
///
/// For now this supports only account-id based addressing. Future schemes (e.g. public keys)
/// can be added as new enum variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    AccountId(AccountIdAddress),
}

/// Address that targets a specific `AccountId` with an explicit tag length preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdAddress {
    id: AccountId,
    tag_len: u8,
}

impl AccountIdAddress {
    /// Creates a new account-id based address with an optional tag length.
    ///
    /// For local (both public and private) accounts, up to 30 bits can be encoded into the tag.
    /// If no `tag_len` is provided, it defaults to [`DEFAULT_LOCAL_TAG_LENGTH`].
    pub fn new(id: AccountId, tag_len: Option<u8>) -> Result<Self, AddressError> {
        if id.storage_mode() == AccountStorageMode::Network {
            return Err(AddressError::NetworkAddressesNotSupported);
        }
        let tag_len = tag_len.unwrap_or(DEFAULT_LOCAL_TAG_LENGTH);
        if tag_len > MAX_LOCAL_TAG_LENGTH {
            return Err(AddressError::TagLengthTooLarge(tag_len));
        }
        Ok(Self { id, tag_len })
    }

    /// Returns the underlying account id.
    pub fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the preferred tag length.
    pub fn tag_len(&self) -> u8 {
        self.tag_len
    }
}
