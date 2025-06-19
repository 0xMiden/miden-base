use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::ToString,
    sync::Arc,
    vec::Vec,
};

use assembly::{Assembler, Compile, Library};
use miden_crypto::merkle::InnerNodeInfo;

use super::{AccountInputs, Digest, Felt, Word};
use crate::{
    Hasher, MastForest, MastNodeId, TransactionScriptError,
    note::{NoteId, NoteRecipient},
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
    vm::{AdviceInputs, AdviceMap, Program},
};

// TRANSACTION SCRIPT
// ================================================================================================

/// Transaction script.
///
/// A transaction script is a program that is executed in a transaction after all input notes
/// have been executed.
///
/// The [TransactionScript] object is composed of:
/// - An executable program defined by a [MastForest] and an associated entrypoint.
/// - A set of transaction script inputs defined by a map of key-value inputs that are loaded into
///   the advice inputs' map such that the transaction script can access them.
/// - A script arguments key defined as an optional [`Digest`]: if present, this key will be pushed
///   to the operand stack before the transaction script execution and could be used to get the
///   script arguments array. See [`TransactionScript::with_args`] for more details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionScript {
    mast: Arc<MastForest>,
    entrypoint: MastNodeId,
    inputs: BTreeMap<Digest, Vec<Felt>>,
    args_key: Option<Digest>,
}

impl TransactionScript {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new [TransactionScript] instantiated with the provided code and inputs.
    pub fn new(code: Program, inputs: impl IntoIterator<Item = (Word, Vec<Felt>)>) -> Self {
        Self {
            entrypoint: code.entrypoint(),
            mast: code.mast_forest().clone(),
            inputs: inputs.into_iter().map(|(k, v)| (k.into(), v)).collect(),
            args_key: None,
        }
    }

    /// Returns a new [TransactionScript] compiled from the provided source code and inputs using
    /// the specified assembler.
    ///
    /// # Errors
    /// Returns an error if the compilation of the provided source code fails.
    pub fn compile(
        source_code: impl Compile,
        inputs: impl IntoIterator<Item = (Word, Vec<Felt>)>,
        assembler: Assembler,
    ) -> Result<Self, TransactionScriptError> {
        let program = assembler
            .assemble_program(source_code)
            .map_err(TransactionScriptError::AssemblyError)?;
        Ok(Self::new(program, inputs))
    }

    /// Returns a new [TransactionScript] instantiated from the provided components.
    ///
    /// If the `args_key` is present, it's expected that `inputs` map will have it as a key, with
    /// the value being equal to the provided script arguments array. See
    /// [`TransactionScript::with_args`] for more details.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The provided `args_key` is not presented in the `inputs` map.
    ///
    /// # Panics
    /// Panics if the specified entrypoint is not in the provided MAST forest.
    pub fn from_parts(
        mast: Arc<MastForest>,
        entrypoint: MastNodeId,
        inputs: BTreeMap<Digest, Vec<Felt>>,
        args_key: Option<Digest>,
    ) -> Result<Self, TransactionScriptError> {
        assert!(mast.get_node_by_id(entrypoint).is_some());

        // check that provided `args_key` is presented in the `inputs` map
        if let Some(args_key) = args_key {
            if !inputs.contains_key(&args_key) {
                return Err(TransactionScriptError::MissingScriptArgsKeyEntry(args_key));
            }
        }

        Ok(Self { mast, entrypoint, inputs, args_key })
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Sets the `args_key` to the commitment of the provided arguments slice and extends the
    /// `inputs` map with the `COMPUTED_COMMITMENT -> [[script_args]]` entry.
    ///
    /// Script arguments is an optional array of [`Felt`]s which could be easily accessed at the
    /// beginning of the transaction script execution. The commitment of this array (`args_key`) is
    /// automatically pushed to the operand stack at the beginning of the transaction script
    /// execution and the underlying arguments can be accessed using the `adv.push_mapval`
    /// and `adv_push.n` instructions.
    pub fn with_args(mut self, script_args: &[Felt]) -> Result<Self, TransactionScriptError> {
        let args_key = Hasher::hash_elements(script_args);
        let old_map_value = self.inputs.insert(args_key, script_args.to_vec());

        // check that a new map entry will not overwrite an existing one
        if let Some(old_value) = old_map_value {
            if old_value != script_args {
                return Err(TransactionScriptError::ScriptArgsCollision {
                    key: args_key,
                    new_value: script_args.to_vec(),
                    old_value,
                });
            }
        }

        self.args_key = Some(args_key);
        Ok(self)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the [MastForest] backing this transaction script.
    pub fn mast(&self) -> Arc<MastForest> {
        self.mast.clone()
    }

    /// Returns the commitment of this transaction script (i.e., the script's MAST root).
    pub fn root(&self) -> Digest {
        self.mast[self.entrypoint].digest()
    }

    /// Returns a reference to the inputs for this transaction script.
    pub fn inputs(&self) -> &BTreeMap<Digest, Vec<Felt>> {
        &self.inputs
    }

    /// Returns the commitment of the transaction script arguments, or [`None`] if they were not
    /// specified.
    pub fn args_key(&self) -> Option<Digest> {
        self.args_key
    }

    /// Compiles the provided transaction script source, inputs, and libraries into a
    /// [`TransactionScript`].
    ///
    /// This allows the user to compile a transaction against multiple libraries.
    pub fn compile_tx_script<T>(
        inputs: T,
        libraries: Vec<Library>,
        program: &str,
        assembler: Assembler,
    ) -> Result<TransactionScript, TransactionScriptError>
    where
        T: IntoIterator<Item = (Word, Vec<Felt>)>,
    {
        let mut assembler = assembler;

        for lib in libraries {
            assembler = assembler
                .with_library(lib)
                .map_err(|err| TransactionScriptError::AssemblyError(err))?;
        }

        TransactionScript::compile(program, inputs, assembler)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionScript {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.mast.write_into(target);
        target.write_u32(self.entrypoint.as_u32());
        self.inputs.write_into(target);
        self.args_key.write_into(target);
    }
}

impl Deserializable for TransactionScript {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let mast = MastForest::read_from(source)?;
        let entrypoint = MastNodeId::from_u32_safe(source.read_u32()?, &mast)?;
        let inputs = BTreeMap::<Digest, Vec<Felt>>::read_from(source)?;
        let script_args_key = Option::<Digest>::read_from(source)?;

        Self::from_parts(Arc::new(mast), entrypoint, inputs, script_args_key)
            .map_err(|e| DeserializationError::InvalidValue(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use vm_core::{
        AdviceMap,
        utils::{Deserializable, Serializable},
    };

    use crate::transaction::TransactionArgs;

    #[test]
    fn test_tx_args_serialization() {
        let args = TransactionArgs::new(None, None, AdviceMap::default(), std::vec::Vec::default());
        let bytes: std::vec::Vec<u8> = args.to_bytes();
        let decoded = TransactionArgs::read_from_bytes(&bytes).unwrap();

        assert_eq!(args, decoded);
    }
}
