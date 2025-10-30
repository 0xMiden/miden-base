use miden_objects::block::{BlockHeader, ProposedBlock, SignedBlock};
use miden_objects::crypto::dsa::ecdsa_k256_keccak::SecretKey;

use crate::block::errors::BlockHeaderError;
use crate::block::header::construct_block_header;

/// Signs a proposed block using the provided secret key.
///
/// TODO(serge): more docs when impl is locked down.
pub fn sign_block(
    proposed_block: ProposedBlock,
    key: &mut SecretKey,
) -> Result<SignedBlock, BlockHeaderError> {
    let header: BlockHeader = construct_block_header(&proposed_block)?;
    let signature = key.sign(header.commitment()); // TODO(serge): what do we sign?
    Ok(SignedBlock::new(header, proposed_block, signature))
}
