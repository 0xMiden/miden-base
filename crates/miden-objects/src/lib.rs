#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod account;
pub mod address;
pub mod asset;
pub mod batch;
pub mod block;
pub mod note;
pub mod transaction;

#[cfg(any(feature = "testing", test))]
pub mod testing;

mod constants;
mod errors;

// RE-EXPORTS
// ================================================================================================

pub use constants::*;
pub use errors::{
    AccountDeltaError,
    AccountError,
    AccountIdError,
    AccountTreeError,
    AddressError,
    AssetError,
    AssetVaultError,
    BatchAccountUpdateError,
    FeeError,
    NetworkIdError,
    NoteError,
    NullifierTreeError,
    PartialBlockchainError,
    ProposedBatchError,
    ProposedBlockError,
    ProvenBatchError,
    ProvenTransactionError,
    SlotNameError,
    StorageMapError,
    TokenSymbolError,
    TransactionInputError,
    TransactionOutputError,
    TransactionScriptError,
};
pub use miden_core::mast::{MastForest, MastNodeId};
pub use miden_core::prettier::PrettyPrint;
pub use miden_core::{EMPTY_WORD, Felt, FieldElement, ONE, StarkField, WORD_SIZE, ZERO};
pub use miden_crypto::hash::rpo::Rpo256 as Hasher;
pub use miden_crypto::word;
pub use miden_crypto::word::{LexicographicWord, Word, WordError};

pub mod assembly {
    pub use miden_assembly::ast::{Module, ModuleKind, ProcedureName, QualifiedProcedureName};
    pub use miden_assembly::debuginfo::SourceManagerSync;
    pub use miden_assembly::{
        Assembler,
        DefaultSourceManager,
        KernelLibrary,
        Library,
        LibraryNamespace,
        LibraryPath,
        Parse,
        ParseOptions,
        SourceFile,
        SourceId,
        SourceManager,
        SourceSpan,
        debuginfo,
        diagnostics,
        mast,
    };
}

pub mod crypto {
    pub use miden_crypto::{SequentialCommit, dsa, hash, merkle, rand, utils};
    
    // TODO: Replace with actual crypto_box types once available in miden-crypto
    // These are placeholder types for sealed box encryption keys
    
    /// Placeholder for public encryption key used in sealed box encryption.
    /// Will be replaced with `miden_crypto::crypto_box::PublicKey` once available.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PublicEncryptionKey([u8; 32]);
    
    impl PublicEncryptionKey {
        /// Returns the key bytes.
        pub fn as_bytes(&self) -> &[u8; 32] {
            &self.0
        }
        
        /// Creates a key from bytes.
        pub fn from_bytes(bytes: [u8; 32]) -> Self {
            Self(bytes)
        }
    }
    
    impl From<[u8; 32]> for PublicEncryptionKey {
        fn from(bytes: [u8; 32]) -> Self {
            Self(bytes)
        }
    }
    
    /// Placeholder for secret decryption key used in sealed box encryption.
    /// Will be replaced with `miden_crypto::crypto_box::SecretKey` once available.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SecretDecryptionKey([u8; 32]);
    
    impl SecretDecryptionKey {
        /// Generates a random secret key.
        #[cfg(any(feature = "testing", test))]
        pub fn random<R: ::rand::RngCore>(rng: &mut R) -> Self {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            Self(bytes)
        }
        
        /// Returns the corresponding public key.
        pub fn public_key(&self) -> PublicEncryptionKey {
            // Placeholder implementation - will be replaced with actual X25519 derivation
            let mut public_bytes = self.0;
            // Simple transformation for testing - not cryptographically secure
            for byte in &mut public_bytes {
                *byte = byte.wrapping_add(1);
            }
            PublicEncryptionKey(public_bytes)
        }
    }
    
    /// Placeholder seal function for sealed box encryption.
    /// Will be replaced with actual sealed box implementation once available in miden-crypto.
    #[allow(dead_code)]
    pub(crate) fn seal(_plaintext: &[u8], _public_key: &PublicEncryptionKey) -> alloc::vec::Vec<u8> {
        // Placeholder - will be replaced with actual implementation
        unimplemented!("Sealed box encryption not yet available in miden-crypto")
    }
    
    /// Placeholder unseal function for sealed box encryption.
    /// Will be replaced with actual sealed box implementation once available in miden-crypto.
    #[allow(dead_code)]
    pub(crate) fn unseal(
        _ciphertext: &[u8],
        _secret_key: &SecretDecryptionKey,
    ) -> Result<alloc::vec::Vec<u8>, alloc::boxed::Box<dyn core::error::Error + Send + Sync>> {
        // Placeholder - will be replaced with actual implementation
        unimplemented!("Sealed box decryption not yet available in miden-crypto")
    }
}

pub mod utils {
    pub use miden_core::utils::*;
    pub use miden_crypto::utils::{HexParseError, bytes_to_hex_string, hex_to_bytes};
    pub use miden_utils_sync as sync;

    pub mod serde {
        pub use miden_core::utils::{
            ByteReader,
            ByteWriter,
            Deserializable,
            DeserializationError,
            Serializable,
        };
    }
}

pub mod vm {
    pub use miden_assembly_syntax::ast::{AttributeSet, QualifiedProcedureName};
    pub use miden_core::sys_events::SystemEvent;
    pub use miden_core::{AdviceMap, Program, ProgramInfo};
    pub use miden_mast_package::{
        MastArtifact,
        Package,
        PackageExport,
        PackageManifest,
        Section,
        SectionId,
    };
    pub use miden_processor::{AdviceInputs, FutureMaybeSend, RowIndex, StackInputs, StackOutputs};
    pub use miden_verifier::ExecutionProof;
}
