use miden_objects::block::{BlockHeader, ProposedBlock, SignedBlock};
use miden_objects::crypto::dsa::ecdsa_k256_keccak::SecretKey;

use crate::block::header::construct_block_header;

pub mod errors;
pub mod header;

pub fn sign_block(proposed_block: ProposedBlock, key: &mut SecretKey) -> SignedBlock {
    let header: BlockHeader = construct_block_header(proposed_block.clone()).unwrap(); // TODO: no clone? error handling
    let signature = key.sign(header.commitment()); // TODO: what do we sign?
    SignedBlock::new(header, proposed_block, signature)
}
