use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_crypto::dsa::rpo_falcon512::PublicKey;
use miden_crypto::merkle::InnerNodeInfo;
use miden_processor::MastNodeExt;

use super::{AccountInputs, Felt, Hasher, Word};
use crate::note::{NoteId, NoteRecipient};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::vm::{AdviceInputs, AdviceMap, Program};
use crate::{EMPTY_WORD, MastForest, MastNodeId};

// TRANSACTION ARGUMENTS
// ================================================================================================

/// Optional transaction arguments.
///
/// - Transaction script: a program that is executed in a transaction after all input notes scripts
///   have been executed.
/// - Transaction script arguments: a [`Word`], which will be pushed to the operand stack before the
///   transaction script execution. If these arguments are not specified, the [`EMPTY_WORD`] would
///   be used as a default value. If the [AdviceInputs] are propagated with some user defined map
///   entries, this script arguments word could be used as a key to access the corresponding value.
/// - Note arguments: data put onto the stack right before a note script is executed. These are
///   different from note inputs, as the user executing the transaction can specify arbitrary note
///   args.
/// - Advice inputs: provides data needed by the runtime, like the details of public output notes.
/// - Foreign account inputs: provides foreign account data that will be used during the foreign
///   procedure invocation (FPI).
/// - Auth arguments: data put onto the stack right before authentication procedure execution. If
///   this argument is not specified, the [`EMPTY_WORD`] would be used as a default value. If the
///   [AdviceInputs] are propagated with some user defined map entries, this argument could be used
///   as a key to access the corresponding value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionArgs {
    tx_script: Option<TransactionScript>,
    tx_script_args: Word,
    note_args: BTreeMap<NoteId, Word>,
    advice_inputs: AdviceInputs,
    foreign_account_inputs: Vec<AccountInputs>,
    auth_args: Word,
}

impl TransactionArgs {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns new [TransactionArgs] instantiated with the provided transaction script, advice
    /// map and foreign account inputs.
    pub fn new(advice_map: AdviceMap, foreign_account_inputs: Vec<AccountInputs>) -> Self {
        let advice_inputs = AdviceInputs { map: advice_map, ..Default::default() };

        Self {
            tx_script: None,
            tx_script_args: EMPTY_WORD,
            note_args: Default::default(),
            advice_inputs,
            foreign_account_inputs,
            auth_args: EMPTY_WORD,
        }
    }

    /// Returns new [TransactionArgs] instantiated with the provided transaction script.
    ///
    /// If the transaction script is already set, it will be overwritten with the newly provided
    /// one.
    #[must_use]
    pub fn with_tx_script(mut self, tx_script: TransactionScript) -> Self {
        self.tx_script = Some(tx_script);
        self
    }

    /// Returns new [TransactionArgs] instantiated with the provided transaction script and its
    /// arguments.
    ///
    /// If the transaction script and arguments are already set, they will be overwritten with the
    /// newly provided ones.
    #[must_use]
    pub fn with_tx_script_and_args(
        mut self,
        tx_script: TransactionScript,
        tx_script_args: Word,
    ) -> Self {
        self.tx_script = Some(tx_script);
        self.tx_script_args = tx_script_args;
        self
    }

    /// Returns new [TransactionArgs] instantiated with the provided note arguments.
    ///
    /// If the note arguments were already set, they will be overwritten with the newly provided
    /// ones.
    #[must_use]
    pub fn with_note_args(mut self, note_args: BTreeMap<NoteId, Word>) -> Self {
        self.note_args = note_args;
        self
    }

    /// Returns new [TransactionArgs] instantiated with the provided auth arguments.
    #[must_use]
    pub fn with_auth_args(mut self, auth_args: Word) -> Self {
        self.auth_args = auth_args;
        self
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the transaction script.
    pub fn tx_script(&self) -> Option<&TransactionScript> {
        self.tx_script.as_ref()
    }

    /// Returns the transaction script arguments, or [`EMPTY_WORD`] if the arguments were not
    /// specified.
    ///
    /// These arguments could be potentially used as a key to access the advice map during the
    /// transaction script execution. Notice that the corresponding map entry should be provided
    /// separately during the creation with the [`TransactionArgs::new`] or using the
    /// [`TransactionArgs::extend_advice_map`] method.
    pub fn tx_script_args(&self) -> Word {
        self.tx_script_args
    }

    /// Returns a reference to a specific note argument.
    pub fn get_note_args(&self, note_id: NoteId) -> Option<&Word> {
        self.note_args.get(&note_id)
    }

    /// Returns a reference to the internal [AdviceInputs].
    pub fn advice_inputs(&self) -> &AdviceInputs {
        &self.advice_inputs
    }

    /// Returns a reference to the foreign account inputs in the transaction arguments.
    pub fn foreign_account_inputs(&self) -> &[AccountInputs] {
        &self.foreign_account_inputs
    }

    /// Collects and returns a set containing all code commitments from foreign accounts.
    pub fn to_foreign_account_code_commitments(&self) -> BTreeSet<Word> {
        self.foreign_account_inputs()
            .iter()
            .map(|acc| acc.code().commitment())
            .collect()
    }

    /// Returns a reference to the authentication procedure argument, or [`EMPTY_WORD`] if the
    /// argument was not specified.
    ///
    /// This argument could be potentially used as a key to access the advice map during the
    /// transaction script execution. Notice that the corresponding map entry should be provided
    /// separately during the creation with the [`TransactionArgs::new`] or using the
    /// [`TransactionArgs::extend_advice_map`] method.
    pub fn auth_args(&self) -> Word {
        self.auth_args
    }

    // STATE MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Populates the advice inputs with the expected recipient data for creating output notes.
    ///
    /// The advice inputs' map is extended with the following keys:
    ///
    /// - recipient_digest |-> recipient details (inputs_hash, script_root, serial_num).
    /// - inputs_commitment |-> inputs.
    /// - script_root |-> script.
    pub fn add_output_note_recipient<T: AsRef<NoteRecipient>>(&mut self, note_recipient: T) {
        let note_recipient = note_recipient.as_ref();
        let inputs = note_recipient.inputs();
        let script = note_recipient.script();
        let script_encoded: Vec<Felt> = script.into();

        let new_elements = [
            (note_recipient.digest(), note_recipient.format_for_advice()),
            (inputs.commitment(), inputs.format_for_advice()),
            (script.root(), script_encoded),
        ];

        self.advice_inputs.extend(AdviceInputs::default().with_map(new_elements));
    }

    /// Adds the `signature` corresponding to `public_key` on `message` to the advice inputs' map.
    ///
    /// The advice inputs' map is extended with the following key:
    ///
    /// - hash(public_key, message) |-> signature.
    pub fn add_signature(&mut self, public_key: PublicKey, message: Word, signature: Vec<Felt>) {
        self.advice_inputs
            .map
            .insert(Hasher::merge(&[public_key.to_commitment(), message]), signature);
    }

    /// Populates the advice inputs with the specified note recipient details.
    ///
    /// The advice inputs' map is extended with the following keys:
    ///
    /// - recipient |-> recipient details (inputs_hash, script_root, serial_num).
    /// - inputs_commitment |-> inputs.
    /// - script_root |-> script.
    pub fn extend_output_note_recipients<T, L>(&mut self, notes: L)
    where
        L: IntoIterator<Item = T>,
        T: AsRef<NoteRecipient>,
    {
        for note in notes {
            self.add_output_note_recipient(note);
        }
    }

    /// Extends the internal advice inputs' map with the provided key-value pairs.
    pub fn extend_advice_map<T: IntoIterator<Item = (Word, Vec<Felt>)>>(&mut self, iter: T) {
        self.advice_inputs.map.extend(iter);
    }

    /// Extends the internal advice inputs' merkle store with the provided nodes.
    pub fn extend_merkle_store<I: Iterator<Item = InnerNodeInfo>>(&mut self, iter: I) {
        self.advice_inputs.store.extend(iter);
    }

    /// Extends the advice inputs in self with the provided ones.
    #[cfg(feature = "testing")]
    pub fn extend_advice_inputs(&mut self, advice_inputs: AdviceInputs) {
        self.advice_inputs.extend(advice_inputs);
    }
}

impl Serializable for TransactionArgs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.tx_script.write_into(target);
        self.tx_script_args.write_into(target);
        self.note_args.write_into(target);
        self.advice_inputs.write_into(target);
        self.foreign_account_inputs.write_into(target);
        self.auth_args.write_into(target);
    }
}

impl Deserializable for TransactionArgs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tx_script = Option::<TransactionScript>::read_from(source)?;
        let tx_script_args = Word::read_from(source)?;
        let note_args = BTreeMap::<NoteId, Word>::read_from(source)?;
        let advice_inputs = AdviceInputs::read_from(source)?;
        let foreign_account_inputs = Vec::<AccountInputs>::read_from(source)?;
        let auth_args = Word::read_from(source)?;

        Ok(Self {
            tx_script,
            tx_script_args,
            note_args,
            advice_inputs,
            foreign_account_inputs,
            auth_args,
        })
    }
}

// TRANSACTION SCRIPT
// ================================================================================================

/// Transaction script.
///
/// A transaction script is a program that is executed in a transaction after all input notes
/// have been executed.
///
/// The [TransactionScript] object is composed of an executable program defined by a [MastForest]
/// and an associated entrypoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionScript {
    mast: Arc<MastForest>,
    entrypoint: MastNodeId,
}

impl TransactionScript {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new [TransactionScript] instantiated with the provided code.
    pub fn new(code: Program) -> Self {
        Self::from_parts(code.mast_forest().clone(), code.entrypoint())
    }

    /// Returns a new [TransactionScript] instantiated from the provided MAST forest and entrypoint.
    ///
    /// # Panics
    /// Panics if the specified entrypoint is not in the provided MAST forest.
    pub fn from_parts(mast: Arc<MastForest>, entrypoint: MastNodeId) -> Self {
        assert!(mast.get_node_by_id(entrypoint).is_some());

        Self { mast, entrypoint }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the [MastForest] backing this transaction script.
    pub fn mast(&self) -> Arc<MastForest> {
        self.mast.clone()
    }

    /// Returns the commitment of this transaction script (i.e., the script's MAST root).
    pub fn root(&self) -> Word {
        self.mast[self.entrypoint].digest()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionScript {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.mast.write_into(target);
        target.write_u32(self.entrypoint.as_u32());
    }
}

impl Deserializable for TransactionScript {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let mast = MastForest::read_from(source)?;
        let entrypoint = MastNodeId::from_u32_safe(source.read_u32()?, &mast)?;

        Ok(Self::from_parts(Arc::new(mast), entrypoint))
    }
}

#[cfg(test)]
mod tests {
    use miden_core::AdviceMap;
    use miden_core::utils::{Deserializable, Serializable};

    use crate::transaction::TransactionArgs;

    #[test]
    fn test_tx_args_serialization() {
        let tx_args = TransactionArgs::new(AdviceMap::default(), std::vec::Vec::default());
        let bytes: std::vec::Vec<u8> = tx_args.to_bytes();
        let decoded = TransactionArgs::read_from_bytes(&bytes).unwrap();

        assert_eq!(tx_args, decoded);
    }
}
