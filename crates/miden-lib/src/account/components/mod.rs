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

// WELL KNOWN COMPONENTS
// ================================================================================================

/// The enum holding the types of basic well-known account components provided by the `miden-lib`.
pub enum WellKnownComponent {
    BasicWallet,
    BasicFungibleFaucet,
    RpoFalcon512,
    RpoFalcon512Acl,
}

impl WellKnownComponent {
    /// Returns the iterator over procedure digests, containing digests of all procedures provided
    /// by the current component.
    pub fn procedure_digests(&self) -> impl Iterator<Item = Word> {
        let forest = match self {
            Self::BasicWallet => BASIC_WALLET_LIBRARY.mast_forest(),
            Self::BasicFungibleFaucet => BASIC_FUNGIBLE_FAUCET_LIBRARY.mast_forest(),
            Self::RpoFalcon512 => RPO_FALCON_512_LIBRARY.mast_forest(),
            Self::RpoFalcon512Acl => RPO_FALCON_512_ACL_LIBRARY.mast_forest(),
        };

        forest.procedure_digests()
    }

    /// Checks whether all procedures from the current component are present in the procedures map
    /// and if so it removes these procedures from this map and pushes the corresponding component
    /// interface to the component interface vector.
    fn extract_component(
        &self,
        procedures_map: &mut BTreeMap<Word, &AccountProcedureInfo>,
        component_interface_vec: &mut Vec<AccountComponentInterface>,
    ) {
        if self
            .procedure_digests()
            .all(|proc_digest| procedures_map.contains_key(&proc_digest))
        {
            // `storage_offset` is guaranteed to be overwritten because `Self::procedure_digests`
            // will return at least one digest.
            let mut storage_offset = 0u8;
            self.procedure_digests().for_each(|component_procedure| {
                if let Some(proc_info) = procedures_map.remove(&component_procedure) {
                    storage_offset = proc_info.storage_offset();
                }
            });

            match self {
                Self::BasicWallet => {
                    component_interface_vec.push(AccountComponentInterface::BasicWallet)
                },
                Self::BasicFungibleFaucet => component_interface_vec
                    .push(AccountComponentInterface::BasicFungibleFaucet(storage_offset)),
                Self::RpoFalcon512 => component_interface_vec
                    .push(AccountComponentInterface::AuthRpoFalcon512(storage_offset)),
                Self::RpoFalcon512Acl => component_interface_vec
                    .push(AccountComponentInterface::AuthRpoFalcon512Acl(storage_offset)),
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
        Self::RpoFalcon512.extract_component(procedures_map, component_interface_vec);
        Self::RpoFalcon512Acl.extract_component(procedures_map, component_interface_vec);
    }
}
