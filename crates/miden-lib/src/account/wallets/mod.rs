use alloc::string::ToString;

use miden_objects::{
    AccountError, Word,
    account::{Account, AccountBuilder, AccountComponent, AccountStorageMode, AccountType},
};

use super::AuthScheme;
use crate::account::{
    auth::{NoAuth, RpoFalcon512},
    components::basic_wallet_library,
};

// BASIC WALLET
// ================================================================================================

/// An [`AccountComponent`] implementing a basic wallet.
///
/// It reexports the procedures from `miden::contracts::wallets::basic`. When linking against this
/// component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib)) must be available to the
/// assembler which is the case when using [`TransactionKernel::assembler()`][kasm]. The procedures
/// of this component are:
/// - `receive_asset`, which can be used to add an asset to the account.
/// - `move_asset_to_note`, which can be used to remove the specified asset from the account and add
///   it to the output note with the specified index.
///
/// All methods require authentication. Thus, this component must be combined with a component
/// providing authentication.
///
/// This component supports all account types.
///
/// [kasm]: crate::transaction::TransactionKernel::assembler
pub struct BasicWallet;

impl From<BasicWallet> for AccountComponent {
    fn from(_: BasicWallet) -> Self {
        AccountComponent::new(basic_wallet_library(), vec![])
          .expect("basic wallet component should satisfy the requirements of a valid account component")
          .with_supports_all_types()
    }
}

/// Creates a new account with basic wallet interface, the specified authentication scheme and the
/// account storage type. Basic wallets can be specified to have either mutable or immutable code.
///
/// The basic wallet interface exposes three procedures:
/// - `receive_asset`, which can be used to add an asset to the account.
/// - `move_asset_to_note`, which can be used to remove the specified asset from the account and add
///   it to the output note with the specified index.
///
/// All methods require authentication. The authentication procedure is defined by the specified
/// authentication scheme.
pub fn create_basic_wallet(
    init_seed: [u8; 32],
    auth_scheme: AuthScheme,
    account_type: AccountType,
    account_storage_mode: AccountStorageMode,
) -> Result<(Account, Word), AccountError> {
    if matches!(account_type, AccountType::FungibleFaucet | AccountType::NonFungibleFaucet) {
        return Err(AccountError::AssumptionViolated(
            "basic wallet accounts cannot have a faucet account type".to_string(),
        ));
    }

    let auth_component: AccountComponent = match auth_scheme {
        AuthScheme::RpoFalcon512 { pub_key } => RpoFalcon512::new(pub_key).into(),
        AuthScheme::NoAuth => NoAuth::new().into(),
    };

    let (account, account_seed) = AccountBuilder::new(init_seed)
        .account_type(account_type)
        .storage_mode(account_storage_mode)
        .with_auth_component(auth_component)
        .with_component(BasicWallet)
        .build()?;

    Ok((account, account_seed))
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use miden_objects::{ONE, crypto::dsa::rpo_falcon512};
    use vm_processor::utils::{Deserializable, Serializable};

    use super::{Account, AccountStorageMode, AccountType, AuthScheme, create_basic_wallet};

    #[test]
    fn test_create_basic_wallet() {
        let pub_key = rpo_falcon512::PublicKey::new([ONE; 4]);
        let wallet = create_basic_wallet(
            [1; 32],
            AuthScheme::RpoFalcon512 { pub_key },
            AccountType::RegularAccountImmutableCode,
            AccountStorageMode::Public,
        );

        wallet.unwrap_or_else(|err| {
            panic!("{}", err);
        });
    }

    #[test]
    fn test_serialize_basic_wallet() {
        let pub_key = rpo_falcon512::PublicKey::new([ONE; 4]);
        let wallet = create_basic_wallet(
            [1; 32],
            AuthScheme::RpoFalcon512 { pub_key },
            AccountType::RegularAccountImmutableCode,
            AccountStorageMode::Public,
        )
        .unwrap()
        .0;

        let bytes = wallet.to_bytes();
        let deserialized_wallet = Account::read_from_bytes(&bytes).unwrap();
        assert_eq!(wallet, deserialized_wallet);
    }
}
