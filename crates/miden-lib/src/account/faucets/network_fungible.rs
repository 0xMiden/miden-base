use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorage,
    AccountStorageMode,
    AccountType,
    StorageSlot,
};
use miden_objects::asset::TokenSymbol;
use miden_objects::{Felt, FieldElement, Word};

use super::{BasicFungibleFaucet, FungibleFaucetError};
use crate::account::AuthScheme;
use crate::account::auth::NoAuth;
use crate::account::components::network_fungible_faucet_library;
use crate::account::interface::{AccountComponentInterface, AccountInterface};
use crate::procedure_digest;

// NETWORK FUNGIBLE FAUCET ACCOUNT COMPONENT
// ================================================================================================

// Initialize the digest of the `distribute` procedure of the Network Fungible Faucet only once.
procedure_digest!(
    NETWORK_FUNGIBLE_FAUCET_DISTRIBUTE,
    NetworkFungibleFaucet::DISTRIBUTE_PROC_NAME,
    network_fungible_faucet_library
);

// Initialize the digest of the `burn` procedure of the Network Fungible Faucet only once.
procedure_digest!(
    NETWORK_FUNGIBLE_FAUCET_BURN,
    NetworkFungibleFaucet::BURN_PROC_NAME,
    network_fungible_faucet_library
);

/// An [`AccountComponent`] implementing a network fungible faucet.
///
/// It reexports the procedures from `miden::contracts::faucets::network_fungible`. When linking
/// against this component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib)) must be
/// available to the assembler which is the case when using
/// [`TransactionKernel::assembler()`][kasm]. The procedures of this component are:
/// - `distribute`, which mints an assets and create a note for the provided recipient.
/// - `burn`, which burns the provided asset.
///
/// Both `distribute` and `burn` can only be called from note scripts. `distribute` requires
/// authentication while `burn` does not require authentication and can be called by anyone.
/// Thus, this component must be combined with a component providing authentication.
///
/// This component supports accounts of type [`AccountType::FungibleFaucet`].
///
/// Unlike [`super::BasicFungibleFaucet`], this component uses two storage slots:
/// - First slot: Token metadata `[max_supply, decimals, token_symbol, 0]`
/// - Second slot: Owner account ID as a single Word
///
/// [kasm]: crate::transaction::TransactionKernel::assembler
pub struct NetworkFungibleFaucet {
    faucet: BasicFungibleFaucet,
    owner_account_id: Word,
}

impl NetworkFungibleFaucet {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The maximum number of decimals supported by the component.
    pub const MAX_DECIMALS: u8 = 12;

    const DISTRIBUTE_PROC_NAME: &str = "distribute";
    const BURN_PROC_NAME: &str = "burn";

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NetworkFungibleFaucet`] component from the given pieces of metadata.
    ///
    /// # Errors:
    /// Returns an error if:
    /// - the decimals parameter exceeds maximum value of [`Self::MAX_DECIMALS`].
    /// - the max supply parameter exceeds maximum possible amount for a fungible asset
    ///   ([`miden_objects::asset::FungibleAsset::MAX_AMOUNT`])
    pub fn new(
        symbol: TokenSymbol,
        decimals: u8,
        max_supply: Felt,
        owner_account_id: AccountId,
    ) -> Result<Self, FungibleFaucetError> {
        // Create the basic fungible faucet (this validates the metadata)
        let faucet = BasicFungibleFaucet::new(symbol, decimals, max_supply)?;

        // Convert AccountId to Word representation for storage
        let owner_account_id_word: Word = [
            Felt::new(0),
            Felt::new(0),
            owner_account_id.suffix(),
            owner_account_id.prefix().as_felt(),
        ]
        .into();

        Ok(Self {
            faucet,
            owner_account_id: owner_account_id_word,
        })
    }

    /// Attempts to create a new [`NetworkFungibleFaucet`] component from the associated account
    /// interface and storage.
    ///
    /// # Errors:
    /// Returns an error if:
    /// - the provided [`AccountInterface`] does not contain a
    ///   [`AccountComponentInterface::NetworkFungibleFaucet`] component.
    /// - the decimals parameter exceeds maximum value of [`Self::MAX_DECIMALS`].
    /// - the max supply value exceeds maximum possible amount for a fungible asset of
    ///   [`miden_objects::asset::FungibleAsset::MAX_AMOUNT`].
    /// - the token symbol encoded value exceeds the maximum value of
    ///   [`TokenSymbol::MAX_ENCODED_VALUE`].
    fn try_from_interface(
        interface: AccountInterface,
        storage: &AccountStorage,
    ) -> Result<Self, FungibleFaucetError> {
        for component in interface.components().iter() {
            if let AccountComponentInterface::NetworkFungibleFaucet(offset) = component {
                // obtain metadata from storage using offset provided by NetworkFungibleFaucet
                // interface
                let faucet_metadata = storage
                    .get_item(*offset)
                    .map_err(|_| FungibleFaucetError::InvalidStorageOffset(*offset))?;
                let [max_supply, decimals, token_symbol, _] = *faucet_metadata;

                // obtain owner account ID from the next storage slot
                let owner_account_id: Word = storage
                    .get_item(*offset + 1)
                    .map_err(|_| FungibleFaucetError::InvalidStorageOffset(*offset + 1))?;

                // verify metadata values and create BasicFungibleFaucet
                let token_symbol = TokenSymbol::try_from(token_symbol)
                    .map_err(FungibleFaucetError::InvalidTokenSymbol)?;
                let decimals = decimals.as_int().try_into().map_err(|_| {
                    FungibleFaucetError::TooManyDecimals {
                        actual: decimals.as_int(),
                        max: Self::MAX_DECIMALS,
                    }
                })?;

                let faucet = BasicFungibleFaucet::new(token_symbol, decimals, max_supply)?;

                return Ok(Self { faucet, owner_account_id });
            }
        }

        Err(FungibleFaucetError::NoAvailableInterface)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the symbol of the faucet.
    pub fn symbol(&self) -> TokenSymbol {
        self.faucet.symbol()
    }

    /// Returns the decimals of the faucet.
    pub fn decimals(&self) -> u8 {
        self.faucet.decimals()
    }

    /// Returns the max supply of the faucet.
    pub fn max_supply(&self) -> Felt {
        self.faucet.max_supply()
    }

    /// Returns the owner account ID of the faucet.
    pub fn owner_account_id(&self) -> Word {
        self.owner_account_id
    }

    /// Returns the digest of the `distribute` account procedure.
    pub fn distribute_digest() -> Word {
        *NETWORK_FUNGIBLE_FAUCET_DISTRIBUTE
    }

    /// Returns the digest of the `burn` account procedure.
    pub fn burn_digest() -> Word {
        *NETWORK_FUNGIBLE_FAUCET_BURN
    }
}

impl From<NetworkFungibleFaucet> for AccountComponent {
    fn from(network_faucet: NetworkFungibleFaucet) -> Self {
        // Note: data is stored as [a0, a1, a2, a3] but loaded onto the stack as
        // [a3, a2, a1, a0, ...]
        let metadata = Word::new([
            network_faucet.faucet.max_supply(),
            Felt::from(network_faucet.faucet.decimals()),
            network_faucet.faucet.symbol().into(),
            Felt::ZERO,
        ]);

        // Second storage slot stores the owner account ID
        let owner_slot = StorageSlot::Value(network_faucet.owner_account_id);

        AccountComponent::new(
            network_fungible_faucet_library(),
            vec![StorageSlot::Value(metadata), owner_slot]
        )
            .expect("network fungible faucet component should satisfy the requirements of a valid account component")
            .with_supported_type(AccountType::FungibleFaucet)
    }
}

impl TryFrom<Account> for NetworkFungibleFaucet {
    type Error = FungibleFaucetError;

    fn try_from(account: Account) -> Result<Self, Self::Error> {
        let account_interface = AccountInterface::from(&account);

        NetworkFungibleFaucet::try_from_interface(account_interface, account.storage())
    }
}

impl TryFrom<&Account> for NetworkFungibleFaucet {
    type Error = FungibleFaucetError;

    fn try_from(account: &Account) -> Result<Self, Self::Error> {
        let account_interface = AccountInterface::from(account);

        NetworkFungibleFaucet::try_from_interface(account_interface, account.storage())
    }
}

/// Creates a new faucet account with network fungible faucet interface,
/// account storage type, specified authentication scheme, and provided meta data (token symbol,
/// decimals, max supply, owner account ID).
///
/// The network faucet interface exposes two procedures:
/// - `distribute`, which mints an assets and create a note for the provided recipient.
/// - `burn`, which burns the provided asset.
///
/// Both `distribute` and `burn` can only be called from note scripts. `distribute` requires
/// authentication. The authentication procedure is defined by the specified authentication scheme.
/// `burn` does not require authentication and can be called by anyone.
///
/// The storage layout of the network faucet account is:
/// - Slot 0: Reserved slot for faucets.
/// - Slot 1: Public Key of the authentication component.
/// - Slot 2: [num_tracked_procs, allow_unauthorized_output_notes, allow_unauthorized_input_notes,
///   0].
/// - Slot 3: A map with tracked procedure roots.
/// - Slot 4: Token metadata of the faucet.
/// - Slot 5: Owner account ID.
pub fn create_network_fungible_faucet(
    init_seed: [u8; 32],
    symbol: TokenSymbol,
    decimals: u8,
    max_supply: Felt,
    owner_account_id: AccountId,
    account_storage_mode: AccountStorageMode,
    auth_scheme: AuthScheme,
) -> Result<Account, FungibleFaucetError> {
    let auth_component: AccountComponent = match auth_scheme {
        AuthScheme::NoAuth => NoAuth::new().into(),
        AuthScheme::RpoFalcon512 { .. } => {
            return Err(FungibleFaucetError::UnsupportedAuthScheme(
                "network fungible faucets only support NoAuth authentication scheme".into(),
            ));
        },
        AuthScheme::RpoFalcon512Multisig { .. } => {
            return Err(FungibleFaucetError::UnsupportedAuthScheme(
                "network fungible faucets only support NoAuth authentication scheme".into(),
            ));
        },
        AuthScheme::Unknown => {
            return Err(FungibleFaucetError::UnsupportedAuthScheme(
                "network fungible faucets cannot be created with Unknown authentication scheme"
                    .into(),
            ));
        },
    };

    let account = AccountBuilder::new(init_seed)
        .account_type(AccountType::FungibleFaucet)
        .storage_mode(account_storage_mode)
        .with_auth_component(auth_component)
        .with_component(NetworkFungibleFaucet::new(symbol, decimals, max_supply, owner_account_id)?)
        .build()
        .map_err(FungibleFaucetError::AccountError)?;

    Ok(account)
}
