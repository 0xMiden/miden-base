use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::ToString,
    sync::Arc,
    vec::Vec,
};

use assembly::{Assembler, Compile};
use miden_crypto::merkle::InnerNodeInfo;

use super::{AccountInputs, Digest, Felt, Word, TransactionScript};
use crate::{
    Hasher, MastForest, MastNodeId, TransactionScriptError,
    note::{NoteId, NoteRecipient},
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
    vm::{AdviceInputs, AdviceMap, Program},
};

// TRANSACTION ARGS
// ================================================================================================

/// Optional transaction arguments.
///
/// - Transaction script: a program that is executed in a transaction after all input notes scripts
///   have been executed.
/// - Note arguments: data put onto the stack right before a note script is executed. These are
///   different from note inputs, as the user executing the transaction can specify arbitrary note
///   args.
/// - Advice inputs: Provides data needed by the runtime, like the details of public output notes.
/// - Account inputs: Provides account data that will be accessed in the transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionArgs {
    tx_script: Option<TransactionScript>,
    note_args: BTreeMap<NoteId, Word>,
    advice_inputs: AdviceInputs,
    foreign_account_inputs: Vec<AccountInputs>,
}

impl TransactionArgs {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns new [TransactionArgs] instantiated with the provided transaction script and note
    /// arguments.
    ///
    /// If tx_script is provided, this also adds all mappings from the transaction script inputs
    /// to the advice inputs' map.
    pub fn new(
        tx_script: Option<TransactionScript>,
        note_args: Option<BTreeMap<NoteId, Word>>,
        advice_map: AdviceMap,
        foreign_account_inputs: Vec<AccountInputs>,
    ) -> Self {
        let mut advice_inputs = AdviceInputs::default().with_map(advice_map);
        // add transaction script inputs to the advice inputs' map
        if let Some(ref tx_script) = tx_script {
            advice_inputs
                .extend_map(tx_script.inputs().iter().map(|(hash, input)| (*hash, input.clone())))
        }

        Self {
            tx_script,
            note_args: note_args.unwrap_or_default(),
            advice_inputs,
            foreign_account_inputs,
        }
    }

    /// Returns new [TransactionArgs] instantiated with the provided transaction script.
    #[must_use]
    pub fn with_tx_script(mut self, tx_script: TransactionScript) -> Self {
        self.tx_script = Some(tx_script);
        self
    }

    /// Returns new [TransactionArgs] instantiated with the provided note arguments.
    #[must_use]
    pub fn with_note_args(mut self, note_args: BTreeMap<NoteId, Word>) -> Self {
        self.note_args = note_args;
        self
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the transaction script.
    pub fn tx_script(&self) -> Option<&TransactionScript> {
        self.tx_script.as_ref()
    }

    /// Returns a reference to a specific note argument.
    pub fn get_note_args(&self, note_id: NoteId) -> Option<&Word> {
        self.note_args.get(&note_id)
    }

    /// Returns a reference to the args [AdviceInputs].
    pub fn advice_inputs(&self) -> &AdviceInputs {
        &self.advice_inputs
    }

    /// Returns a reference to the foreign account inputs in the transaction args.
    pub fn foreign_account_inputs(&self) -> &[AccountInputs] {
        &self.foreign_account_inputs
    }

    /// Collects and returns a set containing all code commitments from foreign accounts.
    pub fn foreign_account_code_commitments(&self) -> BTreeSet<Digest> {
        self.foreign_account_inputs()
            .iter()
            .map(|acc| acc.code().commitment())
            .collect()
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

        self.advice_inputs.extend_map(new_elements);
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

    /// Extends the advice inputs in self with the provided ones.
    pub fn extend_advice_inputs(&mut self, advice_inputs: AdviceInputs) {
        self.advice_inputs.extend(advice_inputs);
    }

    /// Extends the internal advice inputs' map with the provided key-value pairs.
    pub fn extend_advice_map<T: IntoIterator<Item = (Digest, Vec<Felt>)>>(&mut self, iter: T) {
        self.advice_inputs.extend_map(iter)
    }

    /// Extends the internal advice inputs' merkle store with the provided nodes.
    pub fn extend_merkle_store<I: Iterator<Item = InnerNodeInfo>>(&mut self, iter: I) {
        self.advice_inputs.extend_merkle_store(iter)
    }
}

impl Serializable for TransactionArgs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.tx_script.write_into(target);
        self.note_args.write_into(target);
        self.advice_inputs.write_into(target);
        self.foreign_account_inputs.write_into(target);
    }
}

impl Deserializable for TransactionArgs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tx_script = Option::<TransactionScript>::read_from(source)?;
        let note_args = BTreeMap::<NoteId, Word>::read_from(source)?;
        let advice_inputs = AdviceInputs::read_from(source)?;
        let foreign_account_inputs = Vec::<AccountInputs>::read_from(source)?;

        Ok(Self {
            tx_script,
            note_args,
            advice_inputs,
            foreign_account_inputs,
        })
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
