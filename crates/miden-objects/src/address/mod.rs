use crate::AddressError;
use crate::account::{AccountId, AccountStorageMode};
use crate::note::NoteTag;

/// A user-facing address in Miden.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Address {
    AccountId(AccountIdAddress),
}

/// Address that targets a specific `AccountId` with an explicit tag length preference.
///
/// The tag length preference determines how many bits of the account ID are encoded into
/// [`NoteTag`]s of notes targeted to this address. This lets the owner of the account choose
/// their level of privacy. A higher tag length makes the account more uniquely identifiable and
/// reduces privacy, while a shorter length increases privacy at the cost of matching more notes
/// published onchain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountIdAddress {
    id: AccountId,
    tag_len: u8,
}

impl AccountIdAddress {
    /// Creates a new account ID based address with the default tag length.
    ///
    /// The tag length defaults to [`NoteTag::DEFAULT_LOCAL_TAG_LENGTH`] for local, and
    /// [`NoteTag::DEFAULT_NETWORK_TAG_LENGTH`] for network accounts.
    pub fn new(id: AccountId) -> Self {
        let tag_len = if id.storage_mode() == AccountStorageMode::Network {
            NoteTag::DEFAULT_NETWORK_TAG_LENGTH
        } else {
            NoteTag::DEFAULT_LOCAL_TAG_LENGTH
        };

        Self { id, tag_len }
    }

    /// Sets a custom tag length for the address, determining how many bits of the account ID
    /// are encoded into [`NoteTag`]s.
    ///
    /// For local (both public and private) accounts, up to 30 bits can be encoded into the tag.
    /// For network accounts, the tag length should always be set to 30 bits.
    ///
    /// # Errors
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

    /// Returns the underlying account id.
    pub fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the preferred tag length.
    pub fn note_tag_len(&self) -> u8 {
        self.tag_len
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
}

impl Address {
    /// Returns a note tag derived from this address.
    pub fn to_note_tag(&self) -> NoteTag {
        match self {
            Address::AccountId(addr) => addr.to_note_tag(),
        }
    }
}
