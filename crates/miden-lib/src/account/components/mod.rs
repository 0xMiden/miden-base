use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::AccountProcedureInfo;
use miden_objects::assembly::Library;
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;

use crate::account::interface::AccountComponentInterface;

// Initialize the Basic Wallet library only once.
static BASIC_WALLET_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/account_components/basic_wallet.masl"));
    Library::read_from_bytes(bytes).expect("Shipped Basic Wallet library is well-formed")
});

// Initialize the Rpo Falcon 512 library only once.
static RPO_FALCON_512_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/account_components/rpo_falcon_512.masl"));
    Library::read_from_bytes(bytes).expect("Shipped Rpo Falcon 512 library is well-formed")
});

// Initialize the Basic Fungible Faucet library only once.
static BASIC_FUNGIBLE_FAUCET_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/basic_fungible_faucet.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Basic Fungible Faucet library is well-formed")
});

// Initialize the Rpo Falcon 512 ACL library only once.
static RPO_FALCON_512_ACL_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/rpo_falcon_512_acl.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Rpo Falcon 512 ACL library is well-formed")
});

// Initialize the NoAuth library only once.
static NO_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/account_components/no_auth.masl"));
    Library::read_from_bytes(bytes).expect("Shipped NoAuth library is well-formed")
});

// Initialize the Multisig Rpo Falcon 512 library only once.
static RPO_FALCON_512_MULTISIG_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/multisig_rpo_falcon_512.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Multisig Rpo Falcon 512 library is well-formed")
});

/// Returns the Basic Wallet Library.
pub fn basic_wallet_library() -> Library {
    BASIC_WALLET_LIBRARY.clone()
}

/// Returns the Basic Fungible Faucet Library.
pub fn basic_fungible_faucet_library() -> Library {
    BASIC_FUNGIBLE_FAUCET_LIBRARY.clone()
}

/// Returns the Rpo Falcon 512 Library.
pub fn rpo_falcon_512_library() -> Library {
    RPO_FALCON_512_LIBRARY.clone()
}

/// Returns the Rpo Falcon 512 ACL Library.
pub fn rpo_falcon_512_acl_library() -> Library {
    RPO_FALCON_512_ACL_LIBRARY.clone()
}

/// Returns the NoAuth Library.
pub fn no_auth_library() -> Library {
    NO_AUTH_LIBRARY.clone()
}

/// Returns the Multisig Library.
pub fn multisig_library() -> Library {
    RPO_FALCON_512_MULTISIG_LIBRARY.clone()
}

// WELL KNOWN COMPONENTS
// ================================================================================================

/// The enum holding the types of basic well-known account components provided by the `miden-lib`.
pub enum WellKnownComponent {
    BasicWallet,
    BasicFungibleFaucet,
    AuthRpoFalcon512,
    AuthRpoFalcon512Acl,
    AuthRpoFalcon512Multisig,
    AuthNoAuth,
}

impl WellKnownComponent {
    /// Returns the iterator over procedure digests, containing digests of all procedures provided
    /// by the current component.
    pub fn procedure_digests(&self) -> impl Iterator<Item = Word> {
        let forest = match self {
            Self::BasicWallet => BASIC_WALLET_LIBRARY.mast_forest(),
            Self::BasicFungibleFaucet => BASIC_FUNGIBLE_FAUCET_LIBRARY.mast_forest(),
            Self::AuthRpoFalcon512 => RPO_FALCON_512_LIBRARY.mast_forest(),
            Self::AuthRpoFalcon512Acl => RPO_FALCON_512_ACL_LIBRARY.mast_forest(),
            Self::AuthRpoFalcon512Multisig => RPO_FALCON_512_MULTISIG_LIBRARY.mast_forest(),
            Self::AuthNoAuth => NO_AUTH_LIBRARY.mast_forest(),
        };

        forest.procedure_digests()
    }

    /// Checks whether procedures from the current component are present in the procedures map
    /// and if so it removes these procedures from this map and pushes the corresponding component
    /// interface to the component interface vector.
    fn extract_component(
        &self,
        procedures_map: &mut BTreeMap<Word, &AccountProcedureInfo>,
        component_interface_vec: &mut Vec<AccountComponentInterface>,
    ) {
        // Determine if this component should be extracted based on procedure matching
        let can_extract = if matches!(self, Self::AuthRpoFalcon512Multisig) {
            // Special case for multisig: The multisig library contains both private procedures
            // (like `assert_new_tx`) and exported procedures (like
            // `auth__tx_rpo_falcon512_multisig`). However, account components only
            // include exported procedures. So we use partial matching - if ANY of the
            // library procedures are found in the account, we consider it a multisig
            // component match.
            self.procedure_digests()
                .any(|proc_digest| procedures_map.contains_key(&proc_digest))
        } else {
            // For all other components, require exact matching - ALL library procedures
            // must be present in the account for it to be considered a match.
            self.procedure_digests()
                .all(|proc_digest| procedures_map.contains_key(&proc_digest))
        };

        if can_extract {
            // Extract the storage offset from any matching procedure
            let mut storage_offset = 0u8;
            self.procedure_digests().for_each(|component_procedure| {
                if let Some(proc_info) = procedures_map.remove(&component_procedure) {
                    storage_offset = proc_info.storage_offset();
                }
            });

            // Create the appropriate component interface
            match self {
                Self::BasicWallet => {
                    component_interface_vec.push(AccountComponentInterface::BasicWallet)
                },
                Self::BasicFungibleFaucet => component_interface_vec
                    .push(AccountComponentInterface::BasicFungibleFaucet(storage_offset)),
                Self::AuthRpoFalcon512 => component_interface_vec
                    .push(AccountComponentInterface::AuthRpoFalcon512(storage_offset)),
                Self::AuthRpoFalcon512Acl => component_interface_vec
                    .push(AccountComponentInterface::AuthRpoFalcon512Acl(storage_offset)),
                Self::AuthRpoFalcon512Multisig => component_interface_vec
                    .push(AccountComponentInterface::AuthRpoFalcon512Multisig(storage_offset)),
                Self::AuthNoAuth => {
                    component_interface_vec.push(AccountComponentInterface::AuthNoAuth)
                },
            }
        }
    }

    /// Gets all well known components which could be constructed from the provided procedures map
    /// and pushes them to the `component_interface_vec`.
    pub fn extract_well_known_components(
        procedures_map: &mut BTreeMap<Word, &AccountProcedureInfo>,
        component_interface_vec: &mut Vec<AccountComponentInterface>,
    ) {
        Self::BasicWallet.extract_component(procedures_map, component_interface_vec);
        Self::BasicFungibleFaucet.extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512.extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512Acl.extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512Multisig.extract_component(procedures_map, component_interface_vec);
        Self::AuthNoAuth.extract_component(procedures_map, component_interface_vec);
    }
}
