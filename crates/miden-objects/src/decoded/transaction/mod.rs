//! Domain construction for decoded transaction messages.

#[cfg(test)]
pub(crate) mod test_utils;

mod core;
pub use core::{
    ProvenTransaction,
    ProvenTransactionError,
    TransactionHeader,
    TransactionHeaderBuildError,
    TransactionId,
    TxAccountUpdate,
    TxAccountUpdateError,
};

mod args;
pub use args::{
    NoteArgument,
    ScriptError,
    TransactionArgs,
    TransactionArgsError,
    TransactionScript,
};

mod notes;
pub use notes::{
    AuthenticatedInputNote,
    InputNote,
    InputNoteCommitment,
    InputNoteError,
    InputNotes,
    InputNotesError,
    OutputNote,
    OutputNoteError,
    PrivateOutputNote,
    PrivateOutputNoteError,
    PublicOutputNote,
    PublicOutputNoteError,
};

mod inputs;
pub use inputs::{
    ForeignAccountSlotName,
    ForeignAccountSlotNameError,
    TransactionInputs,
    TransactionInputsError,
    TransactionInputsV1,
};

mod batch;
pub use batch::{
    BatchAccountUpdate,
    BatchAccountUpdateError,
    ProposedBatch,
    ProposedBatchError,
    ProvenBatch,
    ProvenBatchError,
};
