use crate::AddressError;
use crate::account::AccountId;
use crate::note::NoteTag;

/// A user-facing address in Miden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    AccountId(AccountIdAddress),
}

/// Address that targets a specific `AccountId` with an explicit tag length preference.
///
/// The tag length preference lets the owner of the account choose their level of privacy. A higher
/// tag length makes the account more uniquely identifiable and reduces privacy, while a shorter
/// length increases privacy at the cost of matching more notes published onchain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdAddress {
    id: AccountId,
    tag_len: u8,
}

impl AccountIdAddress {
    /// Creates a new account-id based address with the default tag length.
    ///
    /// For local (both public and private) accounts, up to 30 bits can be encoded into the tag.
    /// The tag length defaults to [`DEFAULT_LOCAL_TAG_LENGTH`].
    pub fn new(id: AccountId) -> Self {
        Self {
            id,
            tag_len: NoteTag::DEFAULT_LOCAL_TAG_LENGTH,
        }
    }

    /// Sets a custom tag length for the address.
    ///
    /// # Errors
    /// Returns an error if the tag length exceeds [`MAX_LOCAL_TAG_LENGTH`].
    pub fn with_tag_len(mut self, tag_len: u8) -> Result<Self, AddressError> {
        if tag_len > NoteTag::MAX_LOCAL_TAG_LENGTH {
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
}

impl Address {
    /// Returns a note tag derived from this address.
    pub fn get_note_tag(&self) -> NoteTag {
        match self {
            Address::AccountId(addr) => NoteTag::from_account_id(addr.id()),
        }
    }
}
