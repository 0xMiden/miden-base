use alloc::vec;
use alloc::vec::Vec;

use miden_core::{Felt, FieldElement, Word};
use miden_protocol::NoteError;
use miden_protocol::account::AccountId;
use miden_protocol::crypto::rand::FeltRng;
use miden_protocol::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};

use crate::claim_script;
use crate::eth_address_format::{EthAddressFormat, EthAmount};
use crate::utils::bytes32_to_felts;

// CLAIM NOTE STRUCTURES
// ================================================================================================

/// Proof data for CLAIM note creation.
/// Contains SMT proofs and root hashes using native types.
pub struct ProofData {
    /// SMT proof for local exit root (bytes32\[_DEPOSIT_CONTRACT_TREE_DEPTH\])
    /// 256 elements, each bytes32 represented as 32-byte array
    pub smt_proof_local_exit_root: Vec<[u8; 32]>,
    /// SMT proof for rollup exit root (bytes32\[_DEPOSIT_CONTRACT_TREE_DEPTH\])
    /// 256 elements, each bytes32 represented as 32-byte array
    pub smt_proof_rollup_exit_root: Vec<[u8; 32]>,
    /// Global index (uint256 as 8 u32 values)
    pub global_index: [u32; 8],
    /// Mainnet exit root hash (bytes32 as 32-byte array)
    pub mainnet_exit_root: [u8; 32],
    /// Rollup exit root hash (bytes32 as 32-byte array)
    pub rollup_exit_root: [u8; 32],
}

/// Leaf data for CLAIM note creation.
/// Contains network, address, amount, and metadata using typed representations.
pub struct LeafData {
    /// Origin network identifier (uint32)
    pub origin_network: u32,
    /// Origin token address
    pub origin_token_address: EthAddressFormat,
    /// Destination network identifier (uint32)
    pub destination_network: u32,
    /// Destination address
    pub destination_address: EthAddressFormat,
    /// Amount of tokens (uint256)
    pub amount: EthAmount,
    /// ABI encoded metadata (fixed size of 8 u32 values)
    pub metadata: [u32; 8],
}

/// Output note data for CLAIM note creation.
/// Contains note-specific data and can use Miden types.
/// TODO: Remove all but target_faucet_account_id
pub struct OutputNoteData {
    /// P2ID note serial number (4 felts as Word)
    pub output_p2id_serial_num: Word,
    /// Target agg faucet account ID (2 felts: prefix and suffix)
    pub target_faucet_account_id: AccountId,
    /// P2ID output note tag
    pub output_note_tag: NoteTag,
}

/// Inputs for creating a CLAIM note.
///
/// This struct groups the core data needed to create a CLAIM note that exactly
/// matches the agglayer claimAsset function signature.
pub struct ClaimNoteInputs {
    /// Proof data containing SMT proofs and root hashes
    pub proof_data: ProofData,
    /// Leaf data containing network, address, amount, and metadata
    pub leaf_data: LeafData,
    /// Output note data containing note-specific information
    pub output_note_data: OutputNoteData,
}

impl TryFrom<ClaimNoteInputs> for NoteInputs {
    type Error = NoteError;

    fn try_from(inputs: ClaimNoteInputs) -> Result<Self, Self::Error> {
        // Validate SMT proof lengths - each should be 256 elements (bytes32 values)
        if inputs.proof_data.smt_proof_local_exit_root.len() != 256 {
            return Err(NoteError::other(alloc::format!(
                "smt_proof_local_exit_root must be exactly 256 elements, got {}",
                inputs.proof_data.smt_proof_local_exit_root.len()
            )));
        }
        if inputs.proof_data.smt_proof_rollup_exit_root.len() != 256 {
            return Err(NoteError::other(alloc::format!(
                "smt_proof_rollup_exit_root must be exactly 256 elements, got {}",
                inputs.proof_data.smt_proof_rollup_exit_root.len()
            )));
        }

        // Total elements:
        //   256 + 256 (proofs)
        // + 8 (global index)
        // + 8 + 8 (exit roots)
        // + 1 + 5 + 1 + 5 (networks + addresses)
        // + 8 + 8 (amount + metadata)
        // + 4 (padding)
        // + 4 (output serial num)
        // + 2 (target faucet account id)
        // + 1 (note tag)
        // = 575
        let mut claim_inputs = Vec::with_capacity(575);

        // 1) PROOF DATA
        //
        // We expect each SMT proof element to be a Solidity `bytes32` carrying a `uint32`
        // (e.g. bytes32(uint256(u32))). If any element doesn't fit in u32, something is wrong
        // and we should fail loudly rather than silently reducing/modifying the value.
        for (i, b) in inputs.proof_data.smt_proof_local_exit_root.iter().enumerate() {
            let felt = bytes32_to_u32_felt_strict(b).map_err(|e| {
                NoteError::other(alloc::format!("invalid SMT proof element at index {i}: {e}"))
            })?;
            claim_inputs.push(felt);
        }
        for (i, b) in inputs.proof_data.smt_proof_rollup_exit_root.iter().enumerate() {
            let felt = bytes32_to_u32_felt_strict(b).map_err(|e| {
                NoteError::other(alloc::format!("invalid SMT proof element at index {i}: {e}"))
            })?;
            claim_inputs.push(felt);
        }

        // globalIndex (uint256 as 8 u32 felts)
        claim_inputs.extend(inputs.proof_data.global_index.iter().map(|&v| Felt::new(v as u64)));

        // mainnetExitRoot (bytes32 as 8 u32 felts)
        let mainnet_exit_root_felts = bytes32_to_felts(&inputs.proof_data.mainnet_exit_root)
            .map_err(|e| {
                NoteError::other(alloc::format!("failed to convert mainnet_exit_root: {}", e))
            })?;
        claim_inputs.extend(mainnet_exit_root_felts);

        // rollupExitRoot (bytes32 as 8 u32 felts)
        let rollup_exit_root_felts = bytes32_to_felts(&inputs.proof_data.rollup_exit_root)
            .map_err(|e| {
                NoteError::other(alloc::format!("failed to convert rollup_exit_root: {}", e))
            })?;
        claim_inputs.extend(rollup_exit_root_felts);

        // 2) LEAF DATA
        claim_inputs.push(Felt::new(inputs.leaf_data.origin_network as u64));

        // originTokenAddress (address as 5 u32 felts)
        claim_inputs.extend(inputs.leaf_data.origin_token_address.to_elements());

        claim_inputs.push(Felt::new(inputs.leaf_data.destination_network as u64));

        // destinationAddress (address as 5 u32 felts)
        claim_inputs.extend(inputs.leaf_data.destination_address.to_elements());

        // amount (uint256 as 8 u32 felts)
        claim_inputs
            .extend(inputs.leaf_data.amount.as_array().iter().map(|&v| Felt::new(v as u64)));

        // metadata (fixed size of 8 felts)
        claim_inputs.extend(inputs.leaf_data.metadata.iter().map(|&v| Felt::new(v as u64)));

        // Keep the same trailing padding as the original implementation.
        claim_inputs.extend(core::iter::repeat_n(Felt::ZERO, 4));

        // 3) OUTPUT NOTE DATA
        claim_inputs.extend(inputs.output_note_data.output_p2id_serial_num);

        // target_faucet_account_id (2 felts: prefix and suffix)
        claim_inputs.push(inputs.output_note_data.target_faucet_account_id.prefix().as_felt());
        claim_inputs.push(inputs.output_note_data.target_faucet_account_id.suffix());

        // output note tag
        claim_inputs.push(Felt::new(inputs.output_note_data.output_note_tag.as_u32() as u64));

        NoteInputs::new(claim_inputs)
    }
}

// CLAIM NOTE CREATION
// ================================================================================================

/// Generates a CLAIM note - a note that instructs an agglayer faucet to validate and mint assets.
///
/// # Parameters
/// - `inputs`: The core inputs for creating the CLAIM note
/// - `sender_account_id`: The account ID of the CLAIM note creator
/// - `rng`: Random number generator for creating the CLAIM note serial number
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_claim_note<R: FeltRng>(
    inputs: ClaimNoteInputs,
    sender_account_id: AccountId,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let note_inputs = NoteInputs::try_from(inputs)?;

    // TODO: Make CLAIM note a Network Note once NoteAttachment PR lands
    let tag = NoteTag::for_local_use_case(0, 0)?;

    let metadata = NoteMetadata::new(
        sender_account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::ZERO,
    )?;

    let recipient = NoteRecipient::new(rng.draw_word(), claim_script(), note_inputs);
    let assets = NoteAssets::new(vec![])?;

    Ok(Note::new(assets, metadata, recipient))
}

// Solidity convention when casting smaller integers to bytes32:
// bytes32(uint256(x)) is left-padded with zeros, so the u32 value sits in the last 4 bytes.
fn bytes32_to_u32_felt_strict(bytes32: &[u8; 32]) -> Result<Felt, &'static str> {
    if bytes32[..28].iter().any(|&b| b != 0) {
        return Err("bytes32 does not fit in u32 (non-zero high bytes)");
    }
    let v = u32::from_be_bytes(bytes32[28..32].try_into().unwrap());
    Ok(Felt::new(v as u64))
}
