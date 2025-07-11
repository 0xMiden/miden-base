use miden_objects::{AccountTreeError, NullifierTreeError, Word};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProvenBlockError {
    #[error("nullifier witness has a different root than the current nullifier tree root")]
    NullifierWitnessRootMismatch(#[source] NullifierTreeError),

    #[error("failed to track account witness")]
    AccountWitnessTracking { source: AccountTreeError },

    #[error("account ID prefix already exists in the tree")]
    AccountIdPrefixDuplicate { source: AccountTreeError },

    #[error(
        "account tree root of the previous block header is {prev_block_account_root} but the root of the partial tree computed from account witnesses is {stale_account_root}, indicating that the witnesses are stale"
    )]
    StaleAccountTreeRoot {
        prev_block_account_root: Word,
        stale_account_root: Word,
    },

    #[error(
        "nullifier tree root of the previous block header is {prev_block_nullifier_root} but the root of the partial tree computed from nullifier witnesses is {stale_nullifier_root}, indicating that the witnesses are stale"
    )]
    StaleNullifierTreeRoot {
        prev_block_nullifier_root: Word,
        stale_nullifier_root: Word,
    },
}
