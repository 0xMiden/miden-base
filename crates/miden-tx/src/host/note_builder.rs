use alloc::{boxed::Box, vec::Vec};

use miden_objects::{
    asset::Asset,
    note::{Note, NoteAssets, NoteInputs, NoteMetadata, NoteRecipient, NoteScript, PartialNote},
};

use super::{AdviceProvider, Digest, Felt, OutputNote, TransactionKernelError};

// OUTPUT NOTE BUILDER
// ================================================================================================

/// Builder of an output note, provided primarily to enable adding assets to a note incrementally.
pub struct OutputNoteBuilder {
    metadata: NoteMetadata,
    assets: NoteAssets,
    recipient_digest: Digest,
    recipient: Option<NoteRecipient>,
}

impl OutputNoteBuilder {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new [OutputNoteBuilder] read from the provided stack state and advice provider.
    ///
    /// The stack is expected to be in the following state:
    ///
    ///   [NOTE_METADATA, RECIPIENT]
    ///
    /// Detailed note info such as assets and recipient (when available) are retrieved from the
    /// advice provider.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Note type specified via the stack is malformed.
    /// - Sender account ID specified via the stack is invalid.
    /// - A combination of note type, sender account ID, and note tag do not form a valid
    ///   [NoteMetadata] object.
    /// - Recipient information in the advice provider is present but is malformed.
    /// - A non-private note is missing recipient details.
    pub fn new<A: AdviceProvider>(
        stack: Vec<Felt>,
        adv_provider: &A,
    ) -> Result<Self, TransactionKernelError> {
        // read note metadata info from the stack and build the metadata object
        let metadata_word = [stack[3], stack[2], stack[1], stack[0]];
        let metadata: NoteMetadata = metadata_word
            .try_into()
            .map_err(TransactionKernelError::MalformedNoteMetadata)?;

        // read recipient digest from the stack and try to build note recipient object if there is
        // enough info available in the advice provider
        let recipient_digest = Digest::new([stack[8], stack[7], stack[6], stack[5]]);
        let recipient = if let Some(data) = adv_provider.get_mapped_values(&recipient_digest) {
            if data.len() != 13 {
                return Err(TransactionKernelError::MalformedRecipientData(data.to_vec()));
            }
            let inputs_commitment = Digest::new([data[1], data[2], data[3], data[4]]);
            let script_root = Digest::new([data[5], data[6], data[7], data[8]]);
            let serial_num = [data[9], data[10], data[11], data[12]];
            let script_data = adv_provider.get_mapped_values(&script_root).unwrap_or(&[]);

            let inputs_data = adv_provider.get_mapped_values(&inputs_commitment);
            let inputs = match inputs_data {
                None => NoteInputs::default(),
                Some(inputs) => {
                    let num_inputs = data[0].as_int() as usize;

                    // There must be at least `num_inputs` elements in the advice provider data,
                    // otherwise it is an error.
                    //
                    // It is possible to have more elements because of padding. The extra elements
                    // will be discarded below, and later their contents will be validated by
                    // computing the commitment and checking against the expected value.
                    if num_inputs > inputs.len() {
                        return Err(TransactionKernelError::TooFewElementsForNoteInputs {
                            specified: num_inputs as u64,
                            actual: inputs.len() as u64,
                        });
                    }

                    NoteInputs::new(inputs[0..num_inputs].to_vec())
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
                TransactionKernelError::MalformedNoteScript {
                    data: script_data.to_vec(),
                    source: Box::new(source),
                }
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
