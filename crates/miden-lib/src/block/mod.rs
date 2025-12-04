use miden_objects::ProposedBlockError;
use miden_objects::block::{BlockBody, ProposedBlock, UnsignedBlockHeader};

use crate::transaction::TransactionKernel;

/// Block building errors.
#[derive(Debug, thiserror::Error)]
pub enum BuildBlockError {
    #[error("processing of proposed block failed")]
    ProposedBlockError(#[source] ProposedBlockError),
    #[error("provided secret key does not match previous block header's public key")]
    KeyMismatch,
}

/// Builds a [`UnsignedBlockHeader`] and [`BlockBody`] by computing the following from the state
/// updates encapsulated by the provided [`ProposedBlock`]:
/// - the account root;
/// - the nullifier root;
/// - the note root;
/// - the transaction commitment; and
/// - the chain commitment.
///
/// The returned block header contains the same public key as the previous block, as provided by the
/// proposed block.
///
/// This functionality is handled here because the block header requires [`TransactionKernel`] for
/// its various commitment fields.
pub fn build_block(
    proposed_block: ProposedBlock,
) -> Result<(UnsignedBlockHeader, BlockBody), BuildBlockError> {
    // Get fields from the proposed block before it is consumed.
    let block_num = proposed_block.block_num();
    let timestamp = proposed_block.timestamp();
    let prev_block_header = proposed_block.prev_block_header().clone();

    // Insert the state commitments of updated accounts into the account tree to compute its new
    // root.
    let new_account_root = proposed_block
        .compute_account_root()
        .map_err(BuildBlockError::ProposedBlockError)?;

    // Insert the created nullifiers into the nullifier tree to compute its new root.
    let new_nullifier_root = proposed_block
        .compute_nullifier_root()
        .map_err(BuildBlockError::ProposedBlockError)?;

    // Compute the root of the block note tree.
    let note_tree = proposed_block.compute_block_note_tree();
    let note_root = note_tree.root();

    // Insert the previous block header into the block partial blockchain to get the new chain
    // commitment.
    let new_chain_commitment = proposed_block.compute_chain_commitment();

    // Construct the block body from the proposed block.
    let body = BlockBody::from(proposed_block);

    // Construct the header.
    let tx_commitment = body.transaction_commitment();
    let prev_block_commitment = prev_block_header.commitment();

    // For now we copy the parameters of the previous header, which means the parameters set on
    // the genesis block will be passed through. Eventually, the contained base fees will be
    // updated based on the demand in the currently proposed block.
    let fee_parameters = prev_block_header.fee_parameters().clone();

    // Currently undefined and reserved for future use.
    // See miden-base/1155.
    let version = 0;
    let tx_kernel_commitment = TransactionKernel.to_commitment();
    let header = UnsignedBlockHeader::new(
        version,
        prev_block_commitment,
        block_num,
        new_chain_commitment,
        new_account_root,
        new_nullifier_root,
        note_root,
        tx_commitment,
        tx_kernel_commitment,
        prev_block_header.public_key().clone(),
        fee_parameters,
        timestamp,
    );
    Ok((header, body))
}
