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
    TokenSymbolError,
    TransactionInputError,
    TransactionOutputError,
    TransactionScriptError,
};
pub use miden_crypto::hash::rpo::Rpo256 as Hasher;
pub use miden_crypto::word;
pub use miden_crypto::word::{LexicographicWord, Word, WordError};
pub use vm_core::mast::{MastForest, MastNodeId};
pub use vm_core::prettier::PrettyPrint;
pub use vm_core::{EMPTY_WORD, Felt, FieldElement, ONE, StarkField, WORD_SIZE, ZERO};

pub mod assembly {
    use alloc::sync::Arc;

    pub use assembly::ast::{Module, ModuleKind, ProcedureName, QualifiedProcedureName};
    use assembly::debuginfo::SourceManagerSync;
    pub use assembly::{
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

    // TODO move to `DefaultSourceManager::arc_dyn_default()`
    pub fn default_source_manager_arc_dyn() -> Arc<dyn SourceManagerSync> {
        Arc::new(DefaultSourceManager::default())
    }
}

pub mod crypto {
    pub use miden_crypto::{SequentialCommit, dsa, hash, merkle, rand, utils};
}

pub mod utils {
    pub use miden_crypto::utils::{HexParseError, bytes_to_hex_string, collections, hex_to_bytes};
    pub use miden_crypto::word::parse_hex_string_as_word;
    pub use miden_utils_sync as sync;
    pub use vm_core::utils::*;

    pub mod serde {
        pub use vm_core::utils::{
            ByteReader,
            ByteWriter,
            Deserializable,
            DeserializationError,
            Serializable,
        };
    }
}

pub mod vm {
    pub use miden_verifier::ExecutionProof;
    pub use vm_core::sys_events::SystemEvent;
    pub use vm_core::{AdviceMap, Program, ProgramInfo};
    pub use vm_processor::{AdviceInputs, FutureMaybeSend, RowIndex, StackInputs, StackOutputs};
}
