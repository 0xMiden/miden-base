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
use miden_objects::assembly::{ProcedureName, QualifiedProcedureName};
use miden_objects::asset::{FungibleAsset, TokenSymbol};
use miden_objects::utils::sync::LazyLock;
use miden_objects::{Felt, FieldElement, Word};

use super::FungibleFaucetError;
use crate::account::AuthScheme;
use crate::account::auth::NoAuth;
use crate::account::components::network_fungible_faucet_library;
use crate::account::interface::{AccountComponentInterface, AccountInterface};

// NETWORK FUNGIBLE FAUCET ACCOUNT COMPONENT
// ================================================================================================

// Initialize the digest of the `distribute` procedure of the Network Fungible Faucet only once.
static NETWORK_FUNGIBLE_FAUCET_DISTRIBUTE: LazyLock<Word> = LazyLock::new(|| {
    let distribute_proc_name = QualifiedProcedureName::new(
        Default::default(),
        ProcedureName::new(NetworkFungibleFaucet::DISTRIBUTE_PROC_NAME)
            .expect("failed to create name for 'distribute' procedure"),
    );
    network_fungible_faucet_library()
        .get_procedure_root_by_name(distribute_proc_name)
        .expect("Network Fungible Faucet should contain 'distribute' procedure")
});

// Initialize the digest of the `burn` procedure of the Network Fungible Faucet only once.
static NETWORK_FUNGIBLE_FAUCET_BURN: LazyLock<Word> = LazyLock::new(|| {
    let burn_proc_name = QualifiedProcedureName::new(
        Default::default(),
        ProcedureName::new(NetworkFungibleFaucet::BURN_PROC_NAME)
            .expect("failed to create name for 'burn' procedure"),
    );
    network_fungible_faucet_library()
        .get_procedure_root_by_name(burn_proc_name)
        .expect("Network Fungible Faucet should contain 'burn' procedure")
});

/// An [`AccountComponent`] implementing a network fungible faucet.
///
/// It reexports the procedures from `miden::contracts::faucets::basic_fungible`. When linking
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
    symbol: TokenSymbol,
    decimals: u8,
    max_supply: Felt,
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
    ///   ([`FungibleAsset::MAX_AMOUNT`])
    pub fn new(
        symbol: TokenSymbol,
        decimals: u8,
        max_supply: Felt,
        owner_account_id: AccountId,
    ) -> Result<Self, FungibleFaucetError> {
        // First check that the metadata is valid.
        if decimals > Self::MAX_DECIMALS {
            return Err(FungibleFaucetError::TooManyDecimals {
                actual: decimals as u64,
                max: Self::MAX_DECIMALS,
            });
        } else if max_supply.as_int() > FungibleAsset::MAX_AMOUNT {
            return Err(FungibleFaucetError::MaxSupplyTooLarge {
                actual: max_supply.as_int(),
                max: FungibleAsset::MAX_AMOUNT,
            });
        }

        // Convert AccountId to Word representation for storage
        let owner_account_id_word: Word = [
            Felt::new(0),
            Felt::new(0),
            Felt::new(owner_account_id.suffix().as_int()),
            owner_account_id.prefix().as_felt(),
        ]
        .into();

        Ok(Self {
            symbol,
            decimals,
            max_supply,
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
    ///   [`FungibleAsset::MAX_AMOUNT`].
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

                // verify metadata values
                let token_symbol = TokenSymbol::try_from(token_symbol)
                    .map_err(FungibleFaucetError::InvalidTokenSymbol)?;
                let decimals = decimals.as_int().try_into().map_err(|_| {
                    FungibleFaucetError::TooManyDecimals {
                        actual: decimals.as_int(),
                        max: Self::MAX_DECIMALS,
                    }
                })?;

                // Convert the Word back to AccountId for the constructor
                // The owner_account_id Word is stored as [0, 0, suffix, prefix]
                let prefix_felt = owner_account_id[3];
                let suffix_felt = owner_account_id[2];
                let account_id = AccountId::try_from([prefix_felt, suffix_felt])
                    .map_err(|_| FungibleFaucetError::InvalidStorageOffset(*offset + 1))?;

                return NetworkFungibleFaucet::new(token_symbol, decimals, max_supply, account_id);
            }
        }

        Err(FungibleFaucetError::NoAvailableInterface)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the symbol of the faucet.
    pub fn symbol(&self) -> TokenSymbol {
        self.symbol
    }

    /// Returns the decimals of the faucet.
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Returns the max supply of the faucet.
    pub fn max_supply(&self) -> Felt {
        self.max_supply
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
    fn from(faucet: NetworkFungibleFaucet) -> Self {
        // Note: data is stored as [a0, a1, a2, a3] but loaded onto the stack as
        // [a3, a2, a1, a0, ...]
        let metadata = Word::new([
            faucet.max_supply,
            Felt::from(faucet.decimals),
            faucet.symbol.into(),
            Felt::ZERO,
        ]);

        // Second storage slot stores the owner account ID
        let owner_slot = StorageSlot::Value(faucet.owner_account_id);

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
