use alloc::string::ToString;

use miden_objects::{
    AccountError, Word,
    account::{Account, AccountBuilder, AccountComponent, AccountStorageMode, AccountType},
    assembly::ProcedureName,
};

use super::AuthScheme;
use crate::account::{auth::RpoFalcon512, components::basic_wallet_library};

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

impl BasicWallet {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------
    const RECEIVE_ASSET_PROC_NAME: &str = "receive_asset";
    const MOVE_ASSET_TO_NOTE_PROC_NAME: &str = "move_asset_to_note";

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the digest of the `receive_asset` wallet procedure.
    pub fn receive_asset_digest() -> Word {
        Self::get_procedure_digest_by_name(Self::RECEIVE_ASSET_PROC_NAME)
    }

    /// Returns the digest of the `move_asset_to_note` wallet procedure.
    pub fn move_asset_to_note_digest() -> Word {
        Self::get_procedure_digest_by_name(Self::MOVE_ASSET_TO_NOTE_PROC_NAME)
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Returns the digest of the basic wallet procedure with the specified name.
    fn get_procedure_digest_by_name(procedure_name: &str) -> Word {
        let proc_name = ProcedureName::new(procedure_name).expect("procedure name should be valid");
        let module = basic_wallet_library()
            .module_infos()
            .next()
            .expect("basic_wallet_library should have exactly one module");
        module.get_procedure_digest_by_name(&proc_name).unwrap_or_else(|| {
            panic!("basic_wallet_library should contain the '{proc_name}' procedure")
        })
    }
}

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

    let auth_component: RpoFalcon512 = match auth_scheme {
        AuthScheme::RpoFalcon512 { pub_key } => RpoFalcon512::new(pub_key),
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

    use miden_objects::{ONE, Word, crypto::dsa::rpo_falcon512};
    use vm_processor::utils::{Deserializable, Serializable};

    use super::{Account, AccountStorageMode, AccountType, AuthScheme, create_basic_wallet};

    #[test]
    fn test_create_basic_wallet() {
        let pub_key = rpo_falcon512::PublicKey::new(Word::from([ONE; 4]));
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
        let pub_key = rpo_falcon512::PublicKey::new(Word::from([ONE; 4]));
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
