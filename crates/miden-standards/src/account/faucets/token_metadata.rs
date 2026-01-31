use miden_protocol::account::{AccountStorage, StorageSlot, StorageSlotName};
use miden_protocol::asset::{FungibleAsset, TokenSymbol};
use miden_protocol::{Felt, FieldElement, Word};

use super::FungibleFaucetError;

// TOKEN METADATA
// ================================================================================================

/// Token metadata for fungible faucet accounts.
///
/// This struct encapsulates the metadata associated with a fungible token faucet:
/// - `token_supply`: The current amount of tokens issued by the faucet.
/// - `max_supply`: The maximum amount of tokens that can be issued.
/// - `decimals`: The number of decimal places for token amounts.
/// - `symbol`: The token symbol.
///
/// The metadata (excluding `token_supply`) is stored in a single storage slot as:
/// `[max_supply, decimals, symbol, 0]`
///
/// The `token_supply` is stored separately in the faucet's reserved sysdata slot.
#[derive(Debug, Clone, Copy)]
pub struct TokenMetadata {
    token_supply: Felt,
    max_supply: Felt,
    decimals: u8,
    symbol: TokenSymbol,
}

impl TokenMetadata {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The maximum number of decimals supported.
    pub const MAX_DECIMALS: u8 = 12;

    /// Index of the token issuance element in the faucet sysdata slot.
    const ISSUANCE_ELEMENT_INDEX: usize = 3;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TokenMetadata`] with the specified metadata and zero token supply.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The decimals parameter exceeds [`Self::MAX_DECIMALS`].
    /// - The max supply parameter exceeds [`FungibleAsset::MAX_AMOUNT`].
    pub fn new(
        symbol: TokenSymbol,
        decimals: u8,
        max_supply: Felt,
    ) -> Result<Self, FungibleFaucetError> {
        Self::with_supply(symbol, decimals, max_supply, Felt::ZERO)
    }

    /// Creates a new [`TokenMetadata`] with the specified metadata and token supply.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The decimals parameter exceeds [`Self::MAX_DECIMALS`].
    /// - The max supply parameter exceeds [`FungibleAsset::MAX_AMOUNT`].
    pub fn with_supply(
        symbol: TokenSymbol,
        decimals: u8,
        max_supply: Felt,
        token_supply: Felt,
    ) -> Result<Self, FungibleFaucetError> {
        if decimals > Self::MAX_DECIMALS {
            return Err(FungibleFaucetError::TooManyDecimals {
                actual: decimals as u64,
                max: Self::MAX_DECIMALS,
            });
        }

        if max_supply.as_int() > FungibleAsset::MAX_AMOUNT {
            return Err(FungibleFaucetError::MaxSupplyTooLarge {
                actual: max_supply.as_int(),
                max: FungibleAsset::MAX_AMOUNT,
            });
        }

        Ok(Self {
            token_supply,
            max_supply,
            decimals,
            symbol,
        })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`StorageSlotName`] where the token metadata is stored.
    pub fn metadata_slot() -> &'static StorageSlotName {
        &super::METADATA_SLOT_NAME
    }

    /// Returns the current token supply (amount issued).
    pub fn token_supply(&self) -> Felt {
        self.token_supply
    }

    /// Returns the maximum token supply.
    pub fn max_supply(&self) -> Felt {
        self.max_supply
    }

    /// Returns the number of decimals.
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Returns the token symbol.
    pub fn symbol(&self) -> TokenSymbol {
        self.symbol
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    /// Parses token metadata from a Word.
    ///
    /// The Word is expected to be in the format: `[max_supply, decimals, symbol, _]`
    fn try_from_word(word: Word) -> Result<Self, FungibleFaucetError> {
        let [max_supply, decimals, token_symbol, _] = *word;

        let symbol =
            TokenSymbol::try_from(token_symbol).map_err(FungibleFaucetError::InvalidTokenSymbol)?;

        let decimals =
            decimals.as_int().try_into().map_err(|_| FungibleFaucetError::TooManyDecimals {
                actual: decimals.as_int(),
                max: Self::MAX_DECIMALS,
            })?;

        Self::new(symbol, decimals, max_supply)
    }
}

// TRAIT IMPLEMENTATIONS
// ================================================================================================

impl From<TokenMetadata> for Word {
    fn from(metadata: TokenMetadata) -> Self {
        // Note: data is stored as [a0, a1, a2, a3] but loaded onto the stack as
        // [a3, a2, a1, a0, ...]
        Word::new([
            metadata.max_supply,
            Felt::from(metadata.decimals),
            metadata.symbol.into(),
            Felt::ZERO,
        ])
    }
}

impl From<TokenMetadata> for StorageSlot {
    fn from(metadata: TokenMetadata) -> Self {
        StorageSlot::with_value(TokenMetadata::metadata_slot().clone(), metadata.into())
    }
}

impl TryFrom<&StorageSlot> for TokenMetadata {
    type Error = FungibleFaucetError;

    /// Tries to create [`TokenMetadata`] from a storage slot.
    ///
    /// Note: This only reads the metadata from the slot. The `token_supply` will be set to zero
    /// since it's stored in a separate slot. Use [`TryFrom<&AccountStorage>`] to get the full
    /// metadata including current supply.
    fn try_from(slot: &StorageSlot) -> Result<Self, Self::Error> {
        TokenMetadata::try_from_word(slot.value())
    }
}

impl TryFrom<&AccountStorage> for TokenMetadata {
    type Error = FungibleFaucetError;

    /// Tries to create [`TokenMetadata`] from account storage.
    ///
    /// This reads both the metadata slot and the faucet sysdata slot to get the full token
    /// metadata including the current supply.
    fn try_from(storage: &AccountStorage) -> Result<Self, Self::Error> {
        // Read the metadata slot
        let metadata_word = storage.get_item(TokenMetadata::metadata_slot()).map_err(|err| {
            FungibleFaucetError::StorageLookupFailed {
                slot_name: TokenMetadata::metadata_slot().clone(),
                source: err,
            }
        })?;

        // Parse the metadata
        let mut metadata = TokenMetadata::try_from_word(metadata_word)?;

        // Read the token supply from the faucet sysdata slot
        let sysdata_word =
            storage.get_item(AccountStorage::faucet_sysdata_slot()).map_err(|err| {
                FungibleFaucetError::StorageLookupFailed {
                    slot_name: AccountStorage::faucet_sysdata_slot().clone(),
                    source: err,
                }
            })?;

        metadata.token_supply = sysdata_word[Self::ISSUANCE_ELEMENT_INDEX];

        Ok(metadata)
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_protocol::asset::TokenSymbol;
    use miden_protocol::{Felt, FieldElement, Word};

    use super::*;

    #[test]
    fn token_metadata_new() {
        let symbol = TokenSymbol::new("TEST").unwrap();
        let decimals = 8u8;
        let max_supply = Felt::new(1_000_000);

        let metadata = TokenMetadata::new(symbol, decimals, max_supply).unwrap();

        assert_eq!(metadata.symbol(), symbol);
        assert_eq!(metadata.decimals(), decimals);
        assert_eq!(metadata.max_supply(), max_supply);
        assert_eq!(metadata.token_supply(), Felt::ZERO);
    }

    #[test]
    fn token_metadata_with_supply() {
        let symbol = TokenSymbol::new("TEST").unwrap();
        let decimals = 8u8;
        let max_supply = Felt::new(1_000_000);
        let token_supply = Felt::new(500_000);

        let metadata =
            TokenMetadata::with_supply(symbol, decimals, max_supply, token_supply).unwrap();

        assert_eq!(metadata.symbol(), symbol);
        assert_eq!(metadata.decimals(), decimals);
        assert_eq!(metadata.max_supply(), max_supply);
        assert_eq!(metadata.token_supply(), token_supply);
    }

    #[test]
    fn token_metadata_too_many_decimals() {
        let symbol = TokenSymbol::new("TEST").unwrap();
        let decimals = 13u8; // exceeds MAX_DECIMALS
        let max_supply = Felt::new(1_000_000);

        let result = TokenMetadata::new(symbol, decimals, max_supply);
        assert!(matches!(result, Err(FungibleFaucetError::TooManyDecimals { .. })));
    }

    #[test]
    fn token_metadata_max_supply_too_large() {
        use miden_protocol::asset::FungibleAsset;

        let symbol = TokenSymbol::new("TEST").unwrap();
        let decimals = 8u8;
        // FungibleAsset::MAX_AMOUNT is 2^63 - 1, so we use MAX_AMOUNT + 1 to exceed it
        let max_supply = Felt::new(FungibleAsset::MAX_AMOUNT + 1);

        let result = TokenMetadata::new(symbol, decimals, max_supply);
        assert!(matches!(result, Err(FungibleFaucetError::MaxSupplyTooLarge { .. })));
    }

    #[test]
    fn token_metadata_to_word() {
        let symbol = TokenSymbol::new("POL").unwrap();
        let decimals = 2u8;
        let max_supply = Felt::new(123);

        let metadata = TokenMetadata::new(symbol, decimals, max_supply).unwrap();
        let word: Word = metadata.into();

        assert_eq!(word[0], max_supply);
        assert_eq!(word[1], Felt::from(decimals));
        assert_eq!(word[2], symbol.into());
        assert_eq!(word[3], Felt::ZERO);
    }

    #[test]
    fn token_metadata_from_storage_slot() {
        let symbol = TokenSymbol::new("POL").unwrap();
        let decimals = 2u8;
        let max_supply = Felt::new(123);

        let original = TokenMetadata::new(symbol, decimals, max_supply).unwrap();
        let slot: StorageSlot = original.into();

        let restored = TokenMetadata::try_from(&slot).unwrap();

        assert_eq!(restored.symbol(), symbol);
        assert_eq!(restored.decimals(), decimals);
        assert_eq!(restored.max_supply(), max_supply);
        // token_supply is zero when reading from slot (stored separately)
        assert_eq!(restored.token_supply(), Felt::ZERO);
    }
}
