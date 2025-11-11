use alloc::string::String;

use miden_objects::account::{Account, AccountStorage, AccountType, SlotName};
use miden_objects::{AccountError, Felt, TokenSymbolError};
use thiserror::Error;

mod basic_fungible;
mod network_fungible;

pub use basic_fungible::{BasicFungibleFaucet, create_basic_fungible_faucet};
pub use network_fungible::{NetworkFungibleFaucet, create_network_fungible_faucet};

// FUNGIBLE FAUCET
// ================================================================================================

/// Extension trait for fungible faucet accounts. Provides methods to access the fungible faucet
/// account's reserved storage slot.
pub trait FungibleFaucetExt {
    const ISSUANCE_ELEMENT_INDEX: usize;

    /// Returns the amount of tokens (in base units) issued from this fungible faucet.
    ///
    /// # Errors
    /// Returns an error if the account is not a fungible faucet account.
    fn get_token_issuance(&self) -> Result<Felt, FungibleFaucetError>;
}

impl FungibleFaucetExt for Account {
    const ISSUANCE_ELEMENT_INDEX: usize = 3;

    fn get_token_issuance(&self) -> Result<Felt, FungibleFaucetError> {
        if self.account_type() != AccountType::FungibleFaucet {
            return Err(FungibleFaucetError::NotAFungibleFaucetAccount);
        }

        let slot =
            self.storage().get_item(AccountStorage::faucet_metadata_slot()).map_err(|err| {
                FungibleFaucetError::StorageLookupFailed {
                    slot_name: AccountStorage::faucet_metadata_slot().clone(),
                    source: err,
                }
            })?;
        Ok(slot[Self::ISSUANCE_ELEMENT_INDEX])
    }
}

// FUNGIBLE FAUCET ERROR
// ================================================================================================

/// Basic fungible faucet related errors.
#[derive(Debug, Error)]
pub enum FungibleFaucetError {
    #[error("faucet metadata decimals is {actual} which exceeds max value of {max}")]
    TooManyDecimals { actual: u64, max: u8 },
    #[error("faucet metadata max supply is {actual} which exceeds max value of {max}")]
    MaxSupplyTooLarge { actual: u64, max: u64 },
    #[error(
        "account interface provided for faucet creation does not have basic fungible faucet component"
    )]
    NoAvailableInterface,
    #[error("failed to retrieve slot with name {slot_name}")]
    StorageLookupFailed {
        slot_name: SlotName,
        source: AccountError,
    },
    #[error("invalid token symbol")]
    InvalidTokenSymbol(#[source] TokenSymbolError),
    #[error("unsupported authentication scheme: {0}")]
    UnsupportedAuthScheme(String),
    #[error("account creation failed")]
    AccountError(#[source] AccountError),
    #[error("account is not a fungible faucet account")]
    NotAFungibleFaucetAccount,
}
