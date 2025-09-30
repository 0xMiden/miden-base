use alloc::string::String;

use miden_objects::account::{Account, AccountType};
use miden_objects::{AccountError, Felt, TokenSymbolError};
use thiserror::Error;

use crate::transaction::memory::FAUCET_STORAGE_DATA_SLOT;

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
    const ISSUANCE_STORAGE_SLOT: u8;

    /// Returns the amount of tokens (in base units) issued from this fungible faucet.
    ///
    /// # Errors
    /// Returns an error if the account is not a fungible faucet account.
    fn get_token_issuance(&self) -> Result<Felt, FungibleFaucetError>;
}

impl FungibleFaucetExt for Account {
    const ISSUANCE_ELEMENT_INDEX: usize = 3;
    const ISSUANCE_STORAGE_SLOT: u8 = FAUCET_STORAGE_DATA_SLOT;

    fn get_token_issuance(&self) -> Result<Felt, FungibleFaucetError> {
        if self.account_type() != AccountType::FungibleFaucet {
            return Err(FungibleFaucetError::NotAFungibleFaucetAccount);
        }

        let slot = self
            .storage()
            .get_item(Self::ISSUANCE_STORAGE_SLOT)
            .map_err(|_| FungibleFaucetError::InvalidStorageOffset(Self::ISSUANCE_STORAGE_SLOT))?;
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
    #[error("storage offset `{0}` is invalid")]
    InvalidStorageOffset(u8),
    #[error("invalid token symbol")]
    InvalidTokenSymbol(#[source] TokenSymbolError),
    #[error("unsupported authentication scheme: {0}")]
    UnsupportedAuthScheme(String),
    #[error("account creation failed")]
    AccountError(#[source] AccountError),
    #[error("account is not a fungible faucet account")]
    NotAFungibleFaucetAccount,
}
// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use miden_objects::account::{AccountBuilder, AccountStorageMode, AccountType};
    use miden_objects::asset::TokenSymbol;
    use miden_objects::{FieldElement, ONE, Word};

    use super::{BasicFungibleFaucet, Felt, FungibleFaucetError, create_basic_fungible_faucet};
    use crate::AuthScheme;
    use crate::account::auth::{AuthRpoFalcon512, PublicKeyCommitment};
    use crate::account::wallets::BasicWallet;

    #[test]
    fn faucet_contract_creation() {
        let pub_key_word = Word::new([ONE; 4]);
        let auth_scheme: AuthScheme = AuthScheme::RpoFalcon512 { pub_key: pub_key_word.into() };

        // we need to use an initial seed to create the wallet account
        let init_seed: [u8; 32] = [
            90, 110, 209, 94, 84, 105, 250, 242, 223, 203, 216, 124, 22, 159, 14, 132, 215, 85,
            183, 204, 149, 90, 166, 68, 100, 73, 106, 168, 125, 237, 138, 16,
        ];

        let max_supply = Felt::new(123);
        let token_symbol_string = "POL";
        let token_symbol = TokenSymbol::try_from(token_symbol_string).unwrap();
        let decimals = 2u8;
        let storage_mode = AccountStorageMode::Private;

        let faucet_account = create_basic_fungible_faucet(
            init_seed,
            token_symbol,
            decimals,
            max_supply,
            storage_mode,
            auth_scheme,
        )
        .unwrap();

        // The reserved faucet slot should be initialized to an empty word.
        assert_eq!(faucet_account.storage().get_item(0).unwrap(), Word::empty());

        // The falcon auth component is added first so its assigned storage slot for the public key
        // will be 1.
        assert_eq!(faucet_account.storage().get_item(1).unwrap(), pub_key_word);

        // Slot 2 stores [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0]. With 1 tracked procedure (distribute),
        // allow_unauthorized_output_notes=false, and allow_unauthorized_input_notes=true,
        // this should be [1, 0, 1, 0].
        assert_eq!(
            faucet_account.storage().get_item(2).unwrap(),
            [Felt::ONE, Felt::ZERO, Felt::ONE, Felt::ZERO].into()
        );

        // The procedure root map in slot 3 should contain the distribute procedure root.
        let distribute_root = BasicFungibleFaucet::distribute_digest();
        assert_eq!(
            faucet_account
                .storage()
                .get_map_item(3, [Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::ZERO].into())
                .unwrap(),
            distribute_root
        );

        // Check that faucet metadata was initialized to the given values. The faucet component is
        // added second, so its assigned storage slot for the metadata will be 2.
        assert_eq!(
            faucet_account.storage().get_item(4).unwrap(),
            [Felt::new(123), Felt::new(2), token_symbol.into(), Felt::ZERO].into()
        );

        assert!(faucet_account.is_faucet());

        assert_eq!(faucet_account.account_type(), AccountType::FungibleFaucet);

        // Verify the faucet can be extracted and has correct metadata
        let faucet_component = BasicFungibleFaucet::try_from(faucet_account.clone()).unwrap();
        assert_eq!(faucet_component.symbol(), token_symbol);
        assert_eq!(faucet_component.decimals(), decimals);
        assert_eq!(faucet_component.max_supply(), max_supply);
    }

    #[test]
    fn faucet_create_from_account() {
        // prepare the test data
        let mock_word = Word::from([0, 1, 2, 3u32]);
        let mock_public_key = PublicKeyCommitment::from(mock_word);
        let mock_seed = mock_word.as_bytes();

        // valid account
        let token_symbol = TokenSymbol::new("POL").expect("invalid token symbol");
        let faucet_account = AccountBuilder::new(mock_seed)
            .account_type(AccountType::FungibleFaucet)
            .with_component(
                BasicFungibleFaucet::new(token_symbol, 10, Felt::new(100))
                    .expect("failed to create a fungible faucet component"),
            )
            .with_auth_component(AuthRpoFalcon512::new(mock_public_key))
            .build_existing()
            .expect("failed to create wallet account");

        let basic_ff = BasicFungibleFaucet::try_from(faucet_account)
            .expect("basic fungible faucet creation failed");
        assert_eq!(basic_ff.symbol(), token_symbol);
        assert_eq!(basic_ff.decimals(), 10);
        assert_eq!(basic_ff.max_supply(), Felt::new(100));

        // invalid account: basic fungible faucet component is missing
        let invalid_faucet_account = AccountBuilder::new(mock_seed)
            .account_type(AccountType::FungibleFaucet)
            .with_auth_component(AuthRpoFalcon512::new(mock_public_key))
            // we need to add some other component so the builder doesn't fail
            .with_component(BasicWallet)
            .build_existing()
            .expect("failed to create wallet account");

        let err = BasicFungibleFaucet::try_from(invalid_faucet_account)
            .err()
            .expect("basic fungible faucet creation should fail");
        assert_matches!(err, FungibleFaucetError::NoAvailableInterface);
    }

    /// Check that the obtaining of the basic fungible faucet procedure digests does not panic.
    #[test]
    fn get_faucet_procedures() {
        let _distribute_digest = BasicFungibleFaucet::distribute_digest();
        let _burn_digest = BasicFungibleFaucet::burn_digest();
    }
}
