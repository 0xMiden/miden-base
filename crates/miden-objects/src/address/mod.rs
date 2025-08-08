use core::cmp::min;

use crate::account::AccountId;
use crate::note::NoteTag;

/// A user-facing address in Miden.
///
/// For now this supports only account-id based addressing. Future schemes (e.g. public keys)
/// can be added as new enum variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    AccountId(AccountIdAddress),
}

impl Address {
    /// Derives a `NoteTag` from this address according to its scheme.
    ///
    /// The `tag_len` semantics depend on the underlying address scheme. For account-id based
    /// addressing:
    /// - For network accounts, up to 30 MSBs of the account ID may be included.
    /// - For public/private accounts, up to 14 MSBs of the account ID may be included in the
    ///   tag field; the remaining bits are zeroed.
    pub fn to_note_tag(&self) -> NoteTag {
        match self {
            Address::AccountId(addr) => addr.to_note_tag(),
        }
    }
}

/// Address that targets a specific `AccountId` with an explicit tag length preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdAddress {
    id: AccountId,
    tag_len: u8,
}

impl AccountIdAddress {
    /// Creates a new account-id based address with a desired tag length.
    ///
    /// For network accounts, up to 30 bits can be encoded into the tag.
    /// For local (public/private) accounts, up to 14 bits can be encoded into the tag.
    pub fn new(id: AccountId, tag_len: u8) -> Self {
        Self { id, tag_len }
    }

    /// Returns the underlying account id.
    pub fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the preferred tag length.
    pub fn tag_len(&self) -> u8 {
        self.tag_len
    }

    /// Builds a NoteTag from this account-id address using the `tag_len` policy.
    pub fn to_note_tag(&self) -> NoteTag {
        // Extract the top 30 bits of the account id prefix once; all schemes derive from these.
        let prefix_id: u64 = self.id.prefix().into();
        let high_bits_30 = (prefix_id >> 34) & 0x3fff_ffff; // 30 MSBs of the account id

        match self.id.storage_mode() {
            // Network accounts: use up to 30 bits located in the lower 30 bits of the tag payload.
            crate::account::AccountStorageMode::Network => {
                let k = min(self.tag_len as u32, 30) as u32;
                let payload = if k == 0 {
                    0u32
                } else {
                    // Keep the top-k bits in their top positions within the 30-bit field.
                    let mask_k = (((1u64 << k) - 1) << (30 - k)) as u32;
                    (high_bits_30 as u32) & mask_k
                };
                NoteTag::NetworkAccount(payload)
            }
            // Local (public/private) accounts: only 14 bits available in the upper part of the 30-bit field.
            crate::account::AccountStorageMode::Private | crate::account::AccountStorageMode::Public => {
                let k = min(self.tag_len as u32, 14) as u32;
                if k == 0 {
                    // No bits embedded, payload is zero.
                    NoteTag::LocalAny(0)
                } else {
                    // Take the top 14 bits from the 30-bit sequence.
                    let top14 = ((high_bits_30 >> 16) & 0x3fff) as u32; // 14 bits
                    // Keep only the top-k bits of those 14 bits and place them in the 14-bit segment (bits 16..29).
                    let mask_k_in_14 = ((1u32 << k) - 1) << (14 - k);
                    let payload = (top14 & mask_k_in_14) << 16;
                    NoteTag::LocalAny(payload)
                }
            }
        }
    }
}