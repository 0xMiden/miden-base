use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::AccountProcedureInfo;
use miden_objects::assembly::Library;
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;
use miden_processor::MastNodeExt;

use crate::account::interface::AccountComponentInterface;

// Initialize the Basic Wallet library only once.
static BASIC_WALLET_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/account_components/basic_wallet.masl"));
    Library::read_from_bytes(bytes).expect("Shipped Basic Wallet library is well-formed")
});

/// Initialize the ECDSA K256 Keccak library only once.
static ECDSA_K256_KECCAK_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/ecdsa_k256_keccak.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Ecdsa K256 Keccak library is well-formed")
});

/// Initialize the ECDSA K256 Keccak Multisig library only once.
static ECDSA_K256_KECCAK_MULTISIG_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/multisig_ecdsa_k256_keccak.masl"
    ));
    Library::read_from_bytes(bytes)
        .expect("Shipped Multisig Ecdsa K256 Keccak library is well-formed")
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

// Initialize the Network Fungible Faucet library only once.
static NETWORK_FUNGIBLE_FAUCET_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/account_components/network_fungible_faucet.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Network Fungible Faucet library is well-formed")
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

/// Returns the Network Fungible Faucet Library.
pub fn network_fungible_faucet_library() -> Library {
    NETWORK_FUNGIBLE_FAUCET_LIBRARY.clone()
}

/// Returns the ECDSA K256 Keccak Library.
pub fn ecdsa_k256_keccak_library() -> Library {
    ECDSA_K256_KECCAK_LIBRARY.clone()
}

/// Returns the ECDSA K256 Keccak Multisig Library.
pub fn ecdsa_k256_keccak_multisig_library() -> Library {
    ECDSA_K256_KECCAK_MULTISIG_LIBRARY.clone()
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

/// Returns the RPO Falcon 512 Multisig Library.
pub fn rpo_falcon_512_multisig_library() -> Library {
    RPO_FALCON_512_MULTISIG_LIBRARY.clone()
}

// WELL KNOWN COMPONENTS
// ================================================================================================

/// The enum holding the types of basic well-known account components provided by the `miden-lib`.
pub enum WellKnownComponent {
    BasicWallet,
    BasicFungibleFaucet,
    NetworkFungibleFaucet,
    AuthEcdsaK256Keccak,
    AuthEcdsaK256KeccakMultisig,
    AuthRpoFalcon512,
    AuthRpoFalcon512Acl,
    AuthRpoFalcon512Multisig,
    AuthNoAuth,
}

impl WellKnownComponent {
    /// Returns the iterator over digests of all procedures exported from the component.
    pub fn procedure_digests(&self) -> impl Iterator<Item = Word> {
        let library = match self {
            Self::BasicWallet => BASIC_WALLET_LIBRARY.as_ref(),
            Self::BasicFungibleFaucet => BASIC_FUNGIBLE_FAUCET_LIBRARY.as_ref(),
            Self::NetworkFungibleFaucet => NETWORK_FUNGIBLE_FAUCET_LIBRARY.as_ref(),
            Self::AuthEcdsaK256Keccak => ECDSA_K256_KECCAK_LIBRARY.as_ref(),
            Self::AuthEcdsaK256KeccakMultisig => ECDSA_K256_KECCAK_MULTISIG_LIBRARY.as_ref(),
            Self::AuthRpoFalcon512 => RPO_FALCON_512_LIBRARY.as_ref(),
            Self::AuthRpoFalcon512Acl => RPO_FALCON_512_ACL_LIBRARY.as_ref(),
            Self::AuthRpoFalcon512Multisig => RPO_FALCON_512_MULTISIG_LIBRARY.as_ref(),
            Self::AuthNoAuth => NO_AUTH_LIBRARY.as_ref(),
        };

        library.exports().map(|export| {
            library
                .mast_forest()
                .get_node_by_id(export.node)
                .expect("export node not in the forest")
                .digest()
        })
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
        if self
            .procedure_digests()
            .all(|proc_digest| procedures_map.contains_key(&proc_digest))
        {
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
                Self::NetworkFungibleFaucet => component_interface_vec
                    .push(AccountComponentInterface::NetworkFungibleFaucet(storage_offset)),
                Self::AuthEcdsaK256Keccak => component_interface_vec
                    .push(AccountComponentInterface::AuthEcdsaK256Keccak(storage_offset)),
                Self::AuthEcdsaK256KeccakMultisig => component_interface_vec
                    .push(AccountComponentInterface::AuthEcdsaK256KeccakMultisig(storage_offset)),
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
        Self::NetworkFungibleFaucet.extract_component(procedures_map, component_interface_vec);
        Self::AuthEcdsaK256Keccak.extract_component(procedures_map, component_interface_vec);
        Self::AuthEcdsaK256KeccakMultisig
            .extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512.extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512Acl.extract_component(procedures_map, component_interface_vec);
        Self::AuthRpoFalcon512Multisig.extract_component(procedures_map, component_interface_vec);
        Self::AuthNoAuth.extract_component(procedures_map, component_interface_vec);
    }
}
