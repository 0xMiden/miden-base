use miden_objects::asset::Asset;
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    PartialNote,
};
use miden_processor::AdviceProvider;

use super::{OutputNote, Word};
use crate::errors::TransactionKernelError;

// OUTPUT NOTE BUILDER
// ================================================================================================

/// Builder of an output note, provided primarily to enable adding assets to a note incrementally.
#[derive(Debug, Clone)]
pub struct OutputNoteBuilder {
    metadata: NoteMetadata,
    assets: NoteAssets,
    recipient_digest: Word,
    recipient: Option<NoteRecipient>,
}

impl OutputNoteBuilder {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new [OutputNoteBuilder] from the provided metadata, recipient digest, and advice
    /// provider.
    ///
    /// Detailed note info such as assets and recipient (when available) are retrieved from the
    /// advice provider.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Recipient information in the advice provider is present but is malformed.
    /// - A non-private note is missing recipient details.
    pub fn new(
        metadata: NoteMetadata,
        recipient_digest: Word,
        adv_provider: &AdviceProvider,
    ) -> Result<Self, TransactionKernelError> {
        // This method returns an error if the mapped value is not found.
        let recipient = if let Some(data) = adv_provider.get_mapped_values(&recipient_digest) {
            // Support both old format (13 felts) and new format (12 felts) from build_recipient
            let (serial_num, script_root, inputs_commitment, num_inputs) = if data.len() == 13 {
                // Old format: [num_inputs, INPUTS_COMMITMENT, SCRIPT_ROOT, SERIAL_NUM]
                let num_inputs = data[0].as_int() as usize;
                let inputs_commitment = Word::new([data[1], data[2], data[3], data[4]]);
                let script_root = Word::new([data[5], data[6], data[7], data[8]]);
                let serial_num = Word::from([data[9], data[10], data[11], data[12]]);
                (serial_num, script_root, inputs_commitment, num_inputs)
            } else if data.len() == 12 {
                // New format from build_recipient: [SERIAL_NUM, SCRIPT_ROOT, INPUTS_HASH]
                let serial_num = Word::from([data[0], data[1], data[2], data[3]]);
                let script_root = Word::new([data[4], data[5], data[6], data[7]]);
                let inputs_commitment = Word::new([data[8], data[9], data[10], data[11]]);

                // For the new format, we need to get num_inputs from the inputs data itself
                // We'll handle this below when we fetch the inputs
                (serial_num, script_root, inputs_commitment, 0)
            } else {
                return Err(TransactionKernelError::MalformedRecipientData(data.to_vec()));
            };

            let script_data = adv_provider.get_mapped_values(&script_root).unwrap_or(&[]);
            let inputs_data = adv_provider.get_mapped_values(&inputs_commitment);

            let inputs = match inputs_data {
                None => NoteInputs::default(),
                Some(inputs) => {
                    // For new format (12 felts), num_inputs is 0, so we use the actual length
                    let actual_num_inputs = if num_inputs == 0 { inputs.len() } else { num_inputs };

                    // There must be at least `num_inputs` elements in the advice provider data,
                    // otherwise it is an error.
                    //
                    // It is possible to have more elements because of padding. The extra elements
                    // will be discarded below, and later their contents will be validated by
                    // computing the commitment and checking against the expected value.
                    if actual_num_inputs > inputs.len() {
                        return Err(TransactionKernelError::TooFewElementsForNoteInputs {
                            specified: actual_num_inputs as u64,
                            actual: inputs.len() as u64,
                        });
                    }

                    NoteInputs::new(inputs[0..actual_num_inputs].to_vec())
                        .map_err(TransactionKernelError::MalformedNoteInputs)?
                },
            };

            if inputs.commitment() != inputs_commitment {
                return Err(TransactionKernelError::InvalidNoteInputs {
                    expected: inputs_commitment,
                    actual: inputs.commitment(),
                });
            }

            let script = NoteScript::try_from(script_data).map_err(|source| {
                TransactionKernelError::MalformedNoteScript { data: script_data.to_vec(), source }
            })?;
            let recipient = NoteRecipient::new(serial_num, script, inputs);

            Some(recipient)
        } else if metadata.is_private() {
            None
        } else {
            // if there are no recipient details and the note is not private, return an error
            return Err(TransactionKernelError::PublicNoteMissingDetails(
                metadata,
                recipient_digest,
            ));
        };
        Ok(Self {
            metadata,
            recipient_digest,
            recipient,
            assets: NoteAssets::default(),
        })
    }

    // STATE MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Adds the specified asset to the note.
    ///
    /// # Errors
    /// Returns an error if adding the asset to the note fails. This can happen for the following
    /// reasons:
    /// - The same non-fungible asset is already added to the note.
    /// - A fungible asset issued by the same faucet is already added to the note and adding both
    ///   assets together results in an invalid asset.
    /// - Adding the asset to the note will push the list beyond the [NoteAssets::MAX_NUM_ASSETS]
    ///   limit.
    pub fn add_asset(&mut self, asset: Asset) -> Result<(), TransactionKernelError> {
        self.assets
            .add_asset(asset)
            .map_err(TransactionKernelError::FailedToAddAssetToNote)?;
        Ok(())
    }

    /// Converts this builder to an [OutputNote].
    ///
    /// Depending on the available information, this may result in [OutputNote::Full] or
    /// [OutputNote::Partial] notes.
    pub fn build(self) -> OutputNote {
        match self.recipient {
            Some(recipient) => {
                let note = Note::new(self.assets, self.metadata, recipient);
                OutputNote::Full(note)
            },
            None => {
                let note = PartialNote::new(self.metadata, self.recipient_digest, self.assets);
                OutputNote::Partial(note)
            },
        }
    }
}
