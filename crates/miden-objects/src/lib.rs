#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod account;
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
    AccountDeltaError, AccountError, AccountIdError, AccountTreeError, AssetError, AssetVaultError,
    BatchAccountUpdateError, NetworkIdError, NoteError, NullifierTreeError, PartialBlockchainError,
    ProposedBatchError, ProposedBlockError, ProvenBatchError, ProvenTransactionError,
    TokenSymbolError, TransactionInputError, TransactionOutputError, TransactionScriptError,
};
pub use miden_crypto::{
    hash::rpo::Rpo256 as Hasher,
    word,
    word::{LexicographicWord, Word, WordError},
};
pub use vm_core::{
    EMPTY_WORD, Felt, FieldElement, ONE, StarkField, WORD_SIZE, ZERO,
    mast::{MastForest, MastNodeId},
    prettier::PrettyPrint,
};

pub mod assembly {
    pub use assembly::{
        Assembler, DefaultSourceManager, KernelLibrary, Library, LibraryNamespace, LibraryPath,
        Parse, ParseOptions, SourceFile, SourceId, SourceManager,
        ast::{Module, ModuleKind, ProcedureName, QualifiedProcedureName},
        debuginfo, diagnostics, mast,
    };
}

pub mod crypto {
    pub use miden_crypto::{SequentialCommit, dsa, hash, merkle, rand, utils};
}

pub mod utils {
    use alloc::string::String;

    pub use miden_crypto::{
        utils::{HexParseError, bytes_to_hex_string, collections, hex_to_bytes},
        word::parse_hex_string_as_word,
    };
    pub use miden_utils_sync as sync;
    pub use vm_core::utils::*;

    pub mod serde {
        pub use vm_core::utils::{
            ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable,
        };
    }

    /// Converts a word into a string of the word's field elements separated by periods, which can
    /// be used on a MASM `push` instruction to push the word onto the stack.
    ///
    /// # Example
    ///
    /// ```
    /// # use miden_objects::{Word, Felt, utils::word_to_masm_push_string};
    /// let word = Word::from([1, 2, 3, 4u32]);
    /// assert_eq!(word_to_masm_push_string(&word), "1.2.3.4");
    /// ```
    pub fn word_to_masm_push_string(word: &super::Word) -> String {
        format!("{}.{}.{}.{}", word[0], word[1], word[2], word[3])
    }
}

pub mod vm {
    pub use miden_verifier::ExecutionProof;
    pub use vm_core::{AdviceMap, Program, ProgramInfo, sys_events::SystemEvent};
    pub use vm_processor::{AdviceInputs, RowIndex, StackInputs, StackOutputs};
}
