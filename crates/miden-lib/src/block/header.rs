use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use miden_core::Word;
use miden_objects::account::AccountId;
use miden_objects::block::{
    AccountUpdateWitness,
    BlockAccountUpdate,
    BlockBody,
    BlockHeader,
    BlockNumber,
    NullifierWitness,
    OutputNoteBatch,
    ProposedBlock,
};
use miden_objects::note::Nullifier;
use miden_objects::transaction::OrderedTransactionHeaders;

use crate::block::errors::BlockHeaderError;
use crate::transaction::TransactionKernel;

/// Constructs a new [`BlockHeader`] and [`BlockBody`] from the given [`ProposedBlock`].
///
/// Construction of these types is handled here because the block header requires
/// [`TransactionKernel`] for its various commitment fields.
pub fn construct_block(
    mut proposed_block: ProposedBlock,
) -> Result<(BlockHeader, BlockBody), BlockHeaderError> {
    // Get the block number and timestamp of the new block and compute the tx commitment.
    let block_num = proposed_block.block_num();
    let timestamp = proposed_block.timestamp();

    // Insert the state commitments of updated accounts into the account tree to compute its new
    // root.
    let new_account_root = proposed_block.compute_account_root()?;

    // Insert the created nullifiers into the nullifier tree to compute its new root.
    let new_nullifier_root = proposed_block.compute_nullifier_root()?;

    // Compute the root of the block note tree.
    let note_tree = proposed_block.compute_block_note_tree();
    let note_root = note_tree.root();

    // Insert the previous block header into the block partial blockchain to get the new chain
    // commitment.
    let new_chain_commitment = proposed_block.compute_chain_commitment();

    // Split the proposed block into its constituent parts.
    let (
        batches,
        account_updated_witnesses,
        output_note_batches,
        created_nullifiers,
        partial_blockchain,
        prev_block_header,
    ) = proposed_block.into_parts();

    // Aggregate the verified transactions of all batches.
    let transactions = batches.into_transactions();
    let tx_commitment = transactions.commitment();

    let header = construct_block_header(
        block_num,
        timestamp,
        prev_block_header,
        tx_commitment,
        new_chain_commitment,
        new_account_root,
        new_nullifier_root,
        note_root,
    )?;

    let body = construct_block_body(
        account_updated_witnesses,
        created_nullifiers,
        output_note_batches,
        transactions,
    );

    Ok((header, body))
}

// HELPERS
// ================================================================================================

fn construct_block_header(
    block_num: BlockNumber,
    timestamp: u32,
    prev_block_header: BlockHeader,
    tx_commitment: Word,
    new_chain_commitment: Word,
    new_account_root: Word,
    new_nullifier_root: Word,
    note_root: Word,
) -> Result<BlockHeader, BlockHeaderError> {
    let prev_block_commitment = prev_block_header.commitment();

    // For now we copy the parameters of the previous header, which means the parameters set on
    // the genesis block will be passed through. Eventually, the contained base fees will be
    // updated based on the demand in the currently proposed block.
    let fee_parameters = prev_block_header.fee_parameters().clone();

    // Currently undefined and reserved for future use.
    // See miden-base/1155.
    let version = 0;
    let tx_kernel_commitment = TransactionKernel.to_commitment();

    // TODO(serge): remove proof commitment when block header is updated to no longer have it.
    let proof_commitment = Word::empty();

    Ok(BlockHeader::new(
        version,
        prev_block_commitment,
        block_num,
        new_chain_commitment,
        new_account_root,
        new_nullifier_root,
        note_root,
        tx_commitment,
        tx_kernel_commitment,
        proof_commitment,
        fee_parameters,
        timestamp,
    ))
}

fn construct_block_body(
    account_updated_witnesses: Vec<(AccountId, AccountUpdateWitness)>,
    created_nullifiers: BTreeMap<Nullifier, NullifierWitness>,
    output_note_batches: Vec<OutputNoteBatch>,
    transactions: OrderedTransactionHeaders,
) -> BlockBody {
    // Transform the account update witnesses into block account updates.
    let updated_accounts = account_updated_witnesses
        .into_iter()
        .map(|(account_id, update_witness)| {
            let (
                _initial_state_commitment,
                final_state_commitment,
                // Note that compute_account_root took out this value so it should not be used.
                _initial_state_proof,
                details,
            ) = update_witness.into_parts();
            BlockAccountUpdate::new(account_id, final_state_commitment, details)
        })
        .collect();
    let created_nullifiers = created_nullifiers.keys().copied().collect::<Vec<_>>();
    BlockBody::new_unchecked(
        updated_accounts,
        output_note_batches,
        created_nullifiers,
        transactions,
    )
}
