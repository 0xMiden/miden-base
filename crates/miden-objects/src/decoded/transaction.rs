//! Domain construction for decoded transaction messages.
pub use proto::transaction::DecodedTransactionId as TransactionId;

use crate::{BuildUnchecked, ConversionError, DecodeMessage, Verify, proto};

impl Verify for TransactionId {
    type Verified = miden_protocol::transaction::TransactionId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::from_raw(self.id))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionId> for miden_protocol::transaction::TransactionId {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTransactionScript as TransactionScript;

impl Verify for TransactionScript {
    type Verified = miden_protocol::transaction::TransactionScript;
    type Error = ScriptError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let entrypoint = miden_protocol::MastNodeId::from_u32_safe(self.entrypoint, &self.mast)?;
        Self::Verified::from_parts(alloc::sync::Arc::new(self.mast), entrypoint)
            .map_err(|error| ScriptError::Script(alloc::boxed::Box::new(error)))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("invalid script entrypoint: {0}")]
    Entrypoint(#[from] miden_protocol::utils::serde::DeserializationError),
    #[error("invalid transaction script: {0}")]
    // The constructor's error type is not publicly nameable.
    Script(#[source] alloc::boxed::Box<dyn core::error::Error + Send + Sync>),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionScript>
    for miden_protocol::transaction::TransactionScript
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionScript) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedNoteArgument as NoteArgument;

impl Verify for NoteArgument {
    type Verified = (miden_protocol::note::NoteId, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.note_id.verify()?, self.args))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::NoteArgument>
    for (miden_protocol::note::NoteId, miden_protocol::Word)
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::NoteArgument) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTransactionArgs as TransactionArgs;

impl Verify for TransactionArgs {
    type Verified = miden_protocol::transaction::TransactionArgs;
    type Error = TransactionArgsError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let tx_script = self.tx_script.map(Verify::verify).transpose()?;
        let mut note_args = alloc::collections::BTreeMap::new();
        for argument in self.note_args {
            let (id, args) = argument.verify().expect("infallible note argument");
            if note_args.insert(id, args).is_some() {
                return Err(TransactionArgsError::DuplicateNoteArgument(id));
            }
        }
        Ok(Self::Verified::from_parts(
            tx_script,
            self.tx_script_args,
            note_args,
            self.advice_inputs.verify()?,
            self.auth_args,
        ))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TransactionArgsError {
    #[error("invalid transaction script: {0}")]
    Script(#[from] ScriptError),
    #[error("invalid advice inputs: {0}")]
    Advice(#[from] super::primitives::AdviceError),
    #[error("duplicate note argument {0}")]
    DuplicateNoteArgument(miden_protocol::note::NoteId),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionArgs> for miden_protocol::transaction::TransactionArgs {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionArgs) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedForeignAccountSlotName as ForeignAccountSlotName;

impl Verify for ForeignAccountSlotName {
    type Verified =
        (miden_protocol::account::StorageSlotId, miden_protocol::account::StorageSlotName);
    type Error = ForeignAccountSlotNameError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let id = self.slot_id.verify().expect("infallible storage slot ID");
        let name = miden_protocol::account::StorageSlotName::new(self.slot_name)?;
        if name.id() != id {
            return Err(ForeignAccountSlotNameError::IdMismatch {
                expected: name.id(),
                actual: id,
            });
        }
        Ok((id, name))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ForeignAccountSlotNameError {
    #[error("invalid storage slot name: {0}")]
    Name(#[from] miden_protocol::errors::StorageSlotNameError),
    #[error("storage slot ID {actual} does not match the name's ID {expected}")]
    IdMismatch {
        expected: miden_protocol::account::StorageSlotId,
        actual: miden_protocol::account::StorageSlotId,
    },
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::ForeignAccountSlotName>
    for (miden_protocol::account::StorageSlotId, miden_protocol::account::StorageSlotName)
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::ForeignAccountSlotName) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedInputNoteCommitment as InputNoteCommitment;

/// Builds the decoded commitment without checking that its nullifier belongs to its note header.
/// A present header is verified, but the caller must establish nullifier/header consistency and
/// authenticate the note's inclusion separately. An absent header is not evidence of inclusion.
impl BuildUnchecked for InputNoteCommitment {
    type Output = miden_protocol::transaction::InputNoteCommitment;
    type Error = super::note::VerificationError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        Ok(Self::Output::from_parts_unchecked(
            miden_protocol::note::Nullifier::from_raw(self.nullifier),
            self.header.map(Verify::verify).transpose()?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::InputNoteCommitment>
    for miden_protocol::transaction::InputNoteCommitment
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::InputNoteCommitment) -> Result<Self, Self::Error> {
        value.decode_fields()?.build_unchecked().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedPrivateOutputNote as PrivateOutputNote;

impl Verify for PrivateOutputNote {
    type Verified = miden_protocol::transaction::PrivateOutputNote;
    type Error = PrivateOutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.header.verify()?, self.attachments.verify()?)?)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum PrivateOutputNoteError {
    #[error("invalid note: {0}")]
    Note(#[from] super::note::VerificationError),
    #[error("invalid private output note: {0}")]
    Output(#[from] miden_protocol::errors::OutputNoteError),
}
// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::PrivateOutputNote>
    for miden_protocol::transaction::PrivateOutputNote
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::PrivateOutputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTransactionHeader as TransactionHeader;

/// Builds a header using unchecked input-note commitments. Input/output note uniqueness,
/// note-header invariants, and the transmitted transaction ID are still checked. The caller must
/// establish each input's nullifier/header consistency and authentication, the original note order,
/// and the account ID's relationship to the transaction data.
impl BuildUnchecked for TransactionHeader {
    type Output = miden_protocol::transaction::TransactionHeader;
    type Error = TransactionHeaderBuildError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let transmitted = self.transaction_id.verify().expect("infallible transaction ID");
        let input_notes = self
            .input_notes
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<_, _>>()?;
        let input_notes = miden_protocol::transaction::InputNotes::new(input_notes)?;
        let output_notes =
            self.output_notes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        let header = Self::Output::new(
            self.account_id,
            self.initial_state_commitment,
            self.final_state_commitment,
            input_notes,
            output_notes,
        )?;
        if header.id() != transmitted {
            return Err(TransactionHeaderBuildError::IdMismatch {
                transmitted,
                recomputed: header.id(),
            });
        }
        Ok(header)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TransactionHeaderBuildError {
    #[error("invalid note header: {0}")]
    Note(#[from] super::note::VerificationError),
    #[error("invalid input notes: {0}")]
    Input(#[from] miden_protocol::errors::TransactionInputError),
    #[error("invalid transaction header: {0}")]
    Header(#[from] miden_protocol::errors::TransactionHeaderError),
    #[error("transaction ID mismatch: transmitted {transmitted}, recomputed {recomputed}")]
    IdMismatch {
        transmitted: miden_protocol::transaction::TransactionId,
        recomputed: miden_protocol::transaction::TransactionId,
    },
}
// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionHeader>
    for miden_protocol::transaction::TransactionHeader
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionHeader) -> Result<Self, Self::Error> {
        value.decode_fields()?.build_unchecked().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedPublicOutputNote as PublicOutputNote;

impl Verify for PublicOutputNote {
    type Verified = miden_protocol::transaction::PublicOutputNote;
    type Error = PublicOutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.note.verify()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublicOutputNoteError {
    #[error("{0}")]
    Note(#[from] super::note::VerificationError),
    #[error("{0}")]
    Output(#[from] miden_protocol::errors::OutputNoteError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::PublicOutputNote>
    for miden_protocol::transaction::PublicOutputNote
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::PublicOutputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedOutputNote as OutputNote;

impl Verify for OutputNote {
    type Verified = miden_protocol::transaction::OutputNote;
    type Error = OutputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::transaction::output_note::DecodedNote;
        match self.note {
            DecodedNote::Public(note) => Ok(Self::Verified::Public(note.verify()?)),
            DecodedNote::Private(note) => Ok(Self::Verified::Private(note.verify()?)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OutputNoteError {
    #[error("{0}")]
    Public(#[from] PublicOutputNoteError),
    #[error("{0}")]
    Private(#[from] PrivateOutputNoteError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::OutputNote> for miden_protocol::transaction::OutputNote {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::OutputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedAuthenticatedInputNote as AuthenticatedInputNote;

impl Verify for AuthenticatedInputNote {
    type Verified = miden_protocol::transaction::InputNote;
    type Error = InputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let note = self.note.verify()?;
        let (proof_id, proof) = self.proof.verify()?;
        if proof_id != note.id() {
            return Err(InputNoteError::IdMismatch {
                transmitted: proof_id,
                decoded: note.id(),
            });
        }
        Ok(Self::Verified::authenticated(note, proof))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputNoteError {
    #[error("{0}")]
    Note(#[from] super::note::VerificationError),
    #[error("note ID mismatch: transmitted {transmitted}, decoded {decoded}")]
    IdMismatch {
        transmitted: miden_protocol::note::NoteId,
        decoded: miden_protocol::note::NoteId,
    },
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::AuthenticatedInputNote>
    for miden_protocol::transaction::InputNote
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::AuthenticatedInputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedInputNote as InputNote;

impl Verify for InputNote {
    type Verified = miden_protocol::transaction::InputNote;
    type Error = InputNoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::transaction::input_note::DecodedNote;
        match self.note {
            DecodedNote::Authenticated(note) => note.verify(),
            DecodedNote::Unauthenticated(note) => {
                Ok(Self::Verified::unauthenticated(note.verify()?))
            },
        }
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::InputNote> for miden_protocol::transaction::InputNote {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::InputNote) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedInputNotes as InputNotes;

impl Verify for InputNotes {
    type Verified = miden_protocol::transaction::InputNotes<miden_protocol::transaction::InputNote>;
    type Error = InputNotesError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let notes = self.notes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        Ok(Self::Verified::new(notes)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputNotesError {
    #[error("{0}")]
    Note(#[from] InputNoteError),
    #[error("{0}")]
    Input(#[from] miden_protocol::errors::TransactionInputError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::InputNotes>
    for miden_protocol::transaction::InputNotes<miden_protocol::transaction::InputNote>
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::InputNotes) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedTxAccountUpdate as TxAccountUpdate;

impl Verify for TxAccountUpdate {
    type Verified = miden_protocol::transaction::TxAccountUpdate;
    type Error = TxAccountUpdateError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.account_id,
            self.initial_state_commitment,
            self.final_state_commitment,
            self.account_patch_commitment,
            self.details.verify()?,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TxAccountUpdateError {
    #[error("{0}")]
    Details(#[from] super::account::AccountPatchError),
    #[error("{0}")]
    Update(#[from] miden_protocol::errors::ProvenTransactionError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TxAccountUpdate> for miden_protocol::transaction::TxAccountUpdate {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TxAccountUpdate) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedBatchAccountUpdate as BatchAccountUpdate;

impl Verify for BatchAccountUpdate {
    type Verified = miden_protocol::batch::BatchAccountUpdate;
    type Error = BatchAccountUpdateError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.account_id,
            self.initial_state_commitment,
            self.final_state_commitment,
            self.details.verify()?,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BatchAccountUpdateError {
    #[error("{0}")]
    Details(#[from] super::account::AccountPatchError),
    #[error("{0}")]
    Update(#[from] miden_protocol::errors::BatchAccountUpdateError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::BatchAccountUpdate> for miden_protocol::batch::BatchAccountUpdate {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::BatchAccountUpdate) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedProvenTransaction as ProvenTransaction;

/// Checks transaction construction invariants, but not its proof or input-note authentication.
impl crate::BuildUnchecked for ProvenTransaction {
    type Output = miden_protocol::transaction::ProvenTransaction;
    type Error = ProvenTransactionError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let inputs = self
            .input_notes
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<alloc::vec::Vec<_>, _>>()?;
        let outputs = self
            .output_notes
            .into_iter()
            .map(Verify::verify)
            .collect::<Result<alloc::vec::Vec<_>, _>>()?;
        Ok(Self::Output::new(
            self.account_update.verify()?,
            inputs,
            outputs,
            self.reference_block_num.verify().expect("infallible block number"),
            self.reference_block_commitment,
            self.expiration_block_num.verify().expect("infallible block number"),
            self.proof,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProvenTransactionError {
    #[error("{0}")]
    Update(#[from] TxAccountUpdateError),
    #[error("{0}")]
    Input(#[from] super::note::VerificationError),
    #[error("{0}")]
    Output(#[from] OutputNoteError),
    #[error("{0}")]
    Transaction(#[from] miden_protocol::errors::ProvenTransactionError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::ProvenTransaction>
    for miden_protocol::transaction::ProvenTransaction
{
    type Error = ConversionError;
    fn try_from(value: proto::transaction::ProvenTransaction) -> Result<Self, Self::Error> {
        crate::BuildUnchecked::build_unchecked(value.decode_fields()?).map_err(ConversionError::new)
    }
}

pub use proto::transaction::DecodedProposedBatch as ProposedBatch;

impl crate::VerifyWith<u32> for ProposedBatch {
    type Verified = miden_protocol::batch::ProposedBatch;
    type Error = ProposedBatchError;
    fn verify_with(self, proof_security_level: u32) -> Result<Self::Verified, Self::Error> {
        let transactions = self
            .transactions
            .into_iter()
            .map(|tx| tx.build_unchecked().map(alloc::sync::Arc::new))
            .collect::<Result<_, _>>()?;
        let header = self.reference_block_header.build_unchecked()?;
        let chain = self.partial_blockchain.build_unchecked()?;
        let mut proofs = alloc::collections::BTreeMap::new();
        let mut previous = None;
        for proof in self.unauthenticated_note_proofs {
            let (id, proof) = proof.verify()?;
            if previous.is_some_and(|previous| id <= previous) {
                return Err(ProposedBatchError::ProofOrder);
            }
            previous = Some(id);
            proofs.insert(id, proof);
        }
        Ok(Self::Verified::new(transactions, header, chain, proofs, proof_security_level)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProposedBatchError {
    #[error("{0}")]
    Transaction(#[from] ProvenTransactionError),
    #[error("{0}")]
    Header(#[from] super::blockchain::BlockHeaderError),
    #[error("{0}")]
    Chain(#[from] super::blockchain::PartialBlockchainError),
    #[error("{0}")]
    Note(#[from] super::note::VerificationError),
    #[error("unauthenticated note proofs must have unique, ascending note IDs")]
    ProofOrder,
    #[error("{0}")]
    Batch(#[from] miden_protocol::errors::ProposedBatchError),
}

// Compatibility bridge for callers using the combined conversion API.

pub use proto::transaction::DecodedProvenBatch as ProvenBatch;

/// Checks local batch invariants, but not proof validity, note aggregation, or transaction
/// ordering.
impl crate::BuildUnchecked for ProvenBatch {
    type Output = miden_protocol::batch::ProvenBatch;
    type Error = ProvenBatchError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let mut previous = None;
        let mut updates = alloc::vec::Vec::new();
        for update in self.account_updates {
            let update = update.verify()?;
            if previous.is_some_and(|previous| update.account_id() <= previous) {
                return Err(ProvenBatchError::AccountOrder);
            }
            previous = Some(update.account_id());
            updates.push(update);
        }
        let inputs = self
            .input_notes
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<_, _>>()?;
        let outputs =
            self.output_notes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        let transactions = self
            .transactions
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<_, _>>()?;
        Ok(Self::Output::new(
            self.reference_block_commitment,
            self.reference_block_num.verify().expect("infallible block number"),
            updates,
            miden_protocol::transaction::InputNotes::new_unchecked(inputs),
            outputs,
            self.expiration_block_num.verify().expect("infallible block number"),
            miden_protocol::transaction::OrderedTransactionHeaders::new_unchecked(transactions),
            self.proof,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProvenBatchError {
    #[error("{0}")]
    Update(#[from] BatchAccountUpdateError),
    #[error("{0}")]
    Input(#[from] super::note::VerificationError),
    #[error("{0}")]
    Output(#[from] OutputNoteError),
    #[error("{0}")]
    Transaction(#[from] TransactionHeaderBuildError),
    #[error("{0}")]
    Batch(#[from] miden_protocol::errors::ProvenBatchError),
    #[error("account updates must have unique, ascending account IDs")]
    AccountOrder,
    #[error("{0} does not match proposal")]
    ProposalMismatch(&'static str),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::ProvenBatch> for miden_protocol::batch::ProvenBatch {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::ProvenBatch) -> Result<Self, Self::Error> {
        crate::BuildUnchecked::build_unchecked(value.decode_fields()?).map_err(ConversionError::new)
    }
}

/// Checks all fields duplicated from an already-verified proposal. The batch execution proof
/// still needs verification by the consuming service; this only establishes proposal agreement.
impl crate::VerifyWith<&miden_protocol::batch::ProposedBatch> for ProvenBatch {
    type Verified = miden_protocol::batch::ProvenBatch;
    type Error = ProvenBatchError;
    fn verify_with(
        self,
        proposed: &miden_protocol::batch::ProposedBatch,
    ) -> Result<Self::Verified, Self::Error> {
        let batch = self.build_unchecked()?;
        let header = proposed.reference_block_header();
        let mismatch = if batch.reference_block_num() != header.block_num() {
            Some("reference block number")
        } else if batch.reference_block_commitment() != header.commitment() {
            Some("reference block commitment")
        } else if batch.account_updates() != proposed.account_updates() {
            Some("account updates")
        } else if !batch.input_notes().iter().eq(proposed.input_notes().iter()) {
            Some("input notes")
        } else if batch.output_notes() != proposed.output_notes() {
            Some("output notes")
        } else if batch.batch_expiration_block_num() != proposed.batch_expiration_block_num() {
            Some("expiration block")
        } else if batch.transactions().as_slice() != proposed.transaction_headers().as_slice() {
            Some("transaction headers")
        } else {
            None
        };
        if let Some(mismatch) = mismatch {
            return Err(ProvenBatchError::ProposalMismatch(mismatch));
        }
        Ok(batch)
    }
}
