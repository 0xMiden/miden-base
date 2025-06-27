#![no_std]
#![allow(clippy::uninlined_format_args)]

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
pub use miden_crypto::hash::rpo::{Rpo256 as Hasher, RpoDigest as Digest};
pub use vm_core::{
    EMPTY_WORD, Felt, FieldElement, ONE, StarkField, WORD_SIZE, Word, ZERO,
    mast::{MastForest, MastNodeId},
    prettier::PrettyPrint,
};

pub mod assembly {
    pub use assembly::{
        Assembler, AssemblyError, Compile, CompileOptions, DefaultSourceManager, KernelLibrary,
        Library, LibraryNamespace, LibraryPath, SourceFile, SourceId, SourceManager, Version,
        ast::{Module, ModuleKind, ProcedureName, QualifiedProcedureName},
        diagnostics, mast,
    };
}

pub mod crypto {
    pub use miden_crypto::{dsa, hash, merkle, rand, utils};
}

pub mod utils {
    use alloc::string::{String, ToString};

    pub use miden_crypto::utils::{HexParseError, bytes_to_hex_string, collections, hex_to_bytes};
    pub use vm_core::utils::*;
    use vm_core::{Felt, StarkField, Word};

    pub mod serde {
        pub use miden_crypto::utils::{
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
    /// let word = Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);
    /// assert_eq!(word_to_masm_push_string(&word), "1.2.3.4");
    /// ```
    pub fn word_to_masm_push_string(word: &Word) -> String {
        format!("{}.{}.{}.{}", word[0], word[1], word[2], word[3])
    }

    pub const fn parse_hex_string_as_word(hex: &str) -> Result<[Felt; 4], &'static str> {
        const fn parse_hex_digit(digit: u8) -> Result<u8, &'static str> {
            match digit {
                b'0'..=b'9' => Ok(digit - b'0'),
                b'A'..=b'F' => Ok(digit - b'A' + 0x0a),
                b'a'..=b'f' => Ok(digit - b'a' + 0x0a),
                _ => Err("Invalid hex character"),
            }
        }
        // Enforce and skip the '0x' prefix.
        let hex_bytes = match hex.as_bytes() {
            [b'0', b'x', rest @ ..] => rest,
            _ => return Err("Hex string must have a \"0x\" prefix"),
        };

        if hex_bytes.len() > 64 {
            return Err("Hex string has more than 64 characters");
        }

        let mut felts = [0u64; 4];
        let mut i = 0;
        while i < hex_bytes.len() {
            let hex_digit = match parse_hex_digit(hex_bytes[i]) {
                // SAFETY: u8 cast to u64 is safe. We cannot use u64::from in const context so we
                // are forced to cast.
                Ok(v) => v as u64,
                Err(e) => return Err(e),
            };

            // This digit's nibble offset within the felt. We need to invert the nibbles per
            // byte for endianness reasons i.e. ABCD -> BADC.
            let inibble = if i % 2 == 0 { (i + 1) % 16 } else { (i - 1) % 16 };

            let value = hex_digit << (inibble * 4);
            felts[i / 2 / 8] += value;

            i += 1;
        }

        // Ensure each felt is within bounds as `Felt::new` silently wraps around.
        // This matches the behaviour of `Digest::try_from(String)`.
        let mut idx = 0;
        while idx < felts.len() {
            if felts[idx] > Felt::MODULUS {
                return Err("Felt overflow");
            }
            idx += 1;
        }

        Ok([
            Felt::new(felts[0]),
            Felt::new(felts[1]),
            Felt::new(felts[2]),
            Felt::new(felts[3]),
        ])
    }

    /// Construct a new `Digest` from a hex value.
    ///
    /// Expects a '0x' prefixed hex string followed by up to 64 hex digits.
    #[macro_export]
    macro_rules! digest {
        ($hex:expr) => {{
            let felts: [$crate::Felt; 4] = match $crate::utils::parse_hex_string_as_word($hex) {
                Ok(v) => v,
                Err(e) => panic!("{}", e),
            };

            $crate::Digest::new(felts)
        }};
    }

    pub fn parse_hex_to_felts(hex: &str) -> Result<[Felt; 4], String> {
        match parse_hex_string_as_word(hex) {
            Ok(felts) => Ok(felts),
            Err(e) => Err(e.to_string()),
        }
    }

    #[cfg(test)]
    mod tests {
        #[rstest::rstest]
        #[case::missing_prefix("1234")]
        #[case::invalid_character("1234567890abcdefg")]
        #[case::too_long("0xx00000000000000000000000000000000000000000000000000000000000000001")]
        #[case::overflow_felt0(
            "0xffffffffffffffff000000000000000000000000000000000000000000000000"
        )]
        #[case::overflow_felt1(
            "0x0000000000000000ffffffffffffffff00000000000000000000000000000000"
        )]
        #[case::overflow_felt2(
            "0x00000000000000000000000000000000ffffffffffffffff0000000000000000"
        )]
        #[case::overflow_felt3(
            "0x000000000000000000000000000000000000000000000000ffffffffffffffff"
        )]
        #[should_panic]
        fn digest_macro_invalid(#[case] bad_input: &str) {
            digest!(bad_input);
        }

        #[rstest::rstest]
        #[case::each_digit("0x1234567890abcdef")]
        #[case::empty("0x")]
        #[case::zero("0x0")]
        #[case::zero_full("0x0000000000000000000000000000000000000000000000000000000000000000")]
        #[case::one_lsb("0x1")]
        #[case::one_msb("0x0000000000000000000000000000000000000000000000000000000000000001")]
        #[case::one_partial("0x0001")]
        #[case::odd("0x123")]
        #[case::even("0x1234")]
        #[case::touch_each_felt(
            "0x00000000000123450000000000067890000000000000abcd00000000000000ef"
        )]
        #[case::unique_felt("0x111111111111111155555555555555559999999999999999cccccccccccccccc")]
        #[case::digits_on_repeat(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        )]
        fn digest_macro(#[case] input: &str) {
            let uut = digest!(input);

            // Right pad to 64 hex digits (66 including prefix). This is required by the
            // Digest::try_from(String) implementation.
            let padded_input = format!("{input:<66}").replace(" ", "0");
            let expected = crate::Digest::try_from(std::dbg!(padded_input)).unwrap();

            assert_eq!(uut, expected);
        }
    }
}

pub mod vm {
    pub use miden_verifier::ExecutionProof;
    pub use vm_core::{AdviceMap, Program, ProgramInfo, sys_events::SystemEvent};
    pub use vm_processor::{AdviceInputs, RowIndex, StackInputs, StackOutputs};
}
