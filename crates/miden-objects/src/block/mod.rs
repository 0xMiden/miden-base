mod header;
pub use header::{BlockHeader, FeeParameters};

mod block_number;
pub use block_number::BlockNumber;

mod proposed_block;
pub use proposed_block::ProposedBlock;

mod proven_block;
pub use proven_block::ProvenBlock;

mod signed_block;
pub use signed_block::SignedBlock;

mod nullifier_witness;
pub use nullifier_witness::NullifierWitness;

mod partial_account_tree;
pub use partial_account_tree::PartialAccountTree;

pub mod account_tree;

mod nullifier_tree;
pub use nullifier_tree::NullifierTree;

mod blockchain;
pub use blockchain::Blockchain;

mod partial_nullifier_tree;
pub use partial_nullifier_tree::PartialNullifierTree;

mod block_account_update;
pub use block_account_update::BlockAccountUpdate;

mod account_witness;
pub use account_witness::AccountWitness;

mod account_update_witness;
pub use account_update_witness::AccountUpdateWitness;

mod block_inputs;
pub use block_inputs::BlockInputs;

mod note_tree;
pub use note_tree::{BlockNoteIndex, BlockNoteTree};

/// The set of notes created in a transaction batch with their index in the batch.
///
/// The index is included as some notes may be erased at the block level that were part of the
/// output notes of a batch. This means the indices here may not be contiguous, i.e. any missing
/// index belongs to an erased note. To correctly build the [`BlockNoteTree`] of a block, this index
/// is required.
pub type OutputNoteBatch = alloc::vec::Vec<(usize, crate::transaction::OutputNote)>;
