use alloc::vec::Vec;
use core::fmt::Debug;

use miden_crypto::Felt;

use super::{
    ByteReader, ByteWriter, Deserializable, DeserializationError, Hasher, NoteInputs, NoteScript,
    Serializable, Word,
};

/// Value that describes under which condition a note can be consumed.
///
/// The recipient is not an account address, instead it is a value that describes when a note
/// can be consumed. Because not all notes have predetermined consumer addresses, e.g. swap
/// notes can be consumed by anyone, the recipient is defined as the code and its inputs, that
/// when successfully executed results in the note's consumption.
///
/// Recipient is computed as:
///
/// > hash(hash(hash(serial_num, [0; 4]), script_root), input_commitment)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRecipient {
    serial_num: Word,
    script: NoteScript,
    inputs: NoteInputs,
    digest: Word,
}

impl NoteRecipient {
    pub fn new(serial_num: Word, script: NoteScript, inputs: NoteInputs) -> Self {
        let digest = compute_recipient_digest(serial_num, &script, &inputs);
        Self { serial_num, script, inputs, digest }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// The recipient's serial_num, the secret required to consume the note.
    pub fn serial_num(&self) -> Word {
        self.serial_num
    }

    /// The recipients's script which locks the assets of this note.
    pub fn script(&self) -> &NoteScript {
        &self.script
    }

    /// The recipient's inputs which customizes the script's behavior.
    pub fn inputs(&self) -> &NoteInputs {
        &self.inputs
    }

    /// The recipient's digest, which commits to its details.
    ///
    /// This is the public data required to create a note.
    pub fn digest(&self) -> Word {
        self.digest
    }

    /// Returns the recipient formatted to be used with the advice map.
    ///
    /// The format is `inputs_length || INPUTS_COMMITMENT || SCRIPT_ROOT || SERIAL_NUMBER`
    ///
    /// Where:
    /// - inputs_length is the length of the note inputs
    /// - INPUTS_COMMITMENT is the commitment of the note inputs
    /// - SCRIPT_ROOT is the commitment of the note script (i.e., the script's MAST root)
    /// - SERIAL_NUMBER is the recipient's serial number
    pub fn format_for_advice(&self) -> Vec<Felt> {
        let mut result = Vec::with_capacity(13);
        result.push(self.inputs.num_values().into());
        result.extend(self.inputs.commitment());
        result.extend(self.script.root());
        result.extend(self.serial_num);
        result
    }
}

fn compute_recipient_digest(serial_num: Word, script: &NoteScript, inputs: &NoteInputs) -> Word {
    let serial_num_hash = Hasher::merge(&[serial_num.into(), Word::default()]);
    let merge_script = Hasher::merge(&[serial_num_hash, script.root()]);
    Hasher::merge(&[merge_script, inputs.commitment()])
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteRecipient {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let Self {
            script,
            inputs,
            serial_num,

            // These attributes don't have to be serialized, they can be re-computed from the rest
            // of the data
            digest: _,
        } = self;

        script.write_into(target);
        inputs.write_into(target);
        serial_num.write_into(target);
    }
}

impl Deserializable for NoteRecipient {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let script = NoteScript::read_from(source)?;
        let inputs = NoteInputs::read_from(source)?;
        let serial_num = Word::read_from(source)?;

        Ok(Self::new(serial_num, script, inputs))
    }
}
