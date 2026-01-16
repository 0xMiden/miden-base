use alloc::vec;
use alloc::vec::Vec;

use miden_core::{Felt, FieldElement, Word};
use miden_protocol::NoteError;
use miden_protocol::account::AccountId;
use miden_protocol::crypto::SequentialCommit;
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

/// SMT node representation (32-byte hash)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmtNode([u8; 32]);

impl SmtNode {
    /// Creates a new SMT node from a 32-byte array
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the inner 32-byte array
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Converts the SMT node to 8 Felt elements (u256 as 8 u32 values)
    pub fn to_elements(&self) -> Result<[Felt; 8], &'static str> {
        bytes32_to_felts(&self.0)
    }
}

impl From<[u8; 32]> for SmtNode {
    fn from(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}

/// Global exit root representation (32-byte hash)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalExitRoot([u8; 32]);

impl GlobalExitRoot {
    /// Creates a new global exit root from a 32-byte array
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the inner 32-byte array
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Converts the global exit root to 8 Felt elements
    pub fn to_elements(&self) -> Result<[Felt; 8], &'static str> {
        bytes32_to_felts(&self.0)
    }
}

impl From<[u8; 32]> for GlobalExitRoot {
    fn from(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}

/// Proof data for CLAIM note creation.
/// Contains SMT proofs and root hashes using typed representations.
pub struct ProofData {
    /// SMT proof for local exit root (32 SMT nodes)
    pub smt_proof_local_exit_root: [SmtNode; 32],
    /// SMT proof for rollup exit root (32 SMT nodes)
    pub smt_proof_rollup_exit_root: [SmtNode; 32],
    /// Global index (uint256 as 8 u32 values)
    pub global_index: [u32; 8],
    /// Mainnet exit root hash
    pub mainnet_exit_root: GlobalExitRoot,
    /// Rollup exit root hash
    pub rollup_exit_root: GlobalExitRoot,
}

impl ProofData {
    /// Converts the proof data to a vector of field elements for note inputs
    pub fn to_elements(&self) -> Result<Vec<Felt>, NoteError> {
        const PROOF_DATA_ELEMENT_COUNT: usize = 536; // 32*8 + 32*8 + 8 + 8 + 8 (proofs + global_index + 2 exit roots)
        let mut elements = Vec::with_capacity(PROOF_DATA_ELEMENT_COUNT);

        // Convert SMT proof elements to felts (each node is 8 felts)
        for (i, node) in self.smt_proof_local_exit_root.iter().enumerate() {
            let node_felts = node.to_elements().map_err(|e| {
                NoteError::other(alloc::format!(
                    "invalid local exit root SMT proof element at index {i}: {e}"
                ))
            })?;
            elements.extend(node_felts);
        }

        for (i, node) in self.smt_proof_rollup_exit_root.iter().enumerate() {
            let node_felts = node.to_elements().map_err(|e| {
                NoteError::other(alloc::format!(
                    "invalid rollup exit root SMT proof element at index {i}: {e}"
                ))
            })?;
            elements.extend(node_felts);
        }

        // Global index (uint256 as 8 u32 felts)
        elements.extend(self.global_index.iter().map(|&v| Felt::new(v as u64)));

        // Mainnet exit root (bytes32 as 8 u32 felts)
        let mainnet_exit_root_felts = self.mainnet_exit_root.to_elements().map_err(|e| {
            NoteError::other(alloc::format!("failed to convert mainnet_exit_root: {}", e))
        })?;
        elements.extend(mainnet_exit_root_felts);

        // Rollup exit root (bytes32 as 8 u32 felts)
        let rollup_exit_root_felts = self.rollup_exit_root.to_elements().map_err(|e| {
            NoteError::other(alloc::format!("failed to convert rollup_exit_root: {}", e))
        })?;
        elements.extend(rollup_exit_root_felts);

        Ok(elements)
    }
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

impl SequentialCommit for LeafData {
    type Commitment = Word;

    fn to_elements(&self) -> Vec<Felt> {
        const LEAF_DATA_ELEMENT_COUNT: usize = 28; // 1 + 5 + 1 + 5 + 8 + 8 (networks + addresses + amount + metadata)
        let mut elements = Vec::with_capacity(LEAF_DATA_ELEMENT_COUNT);

        // Origin network
        elements.push(Felt::new(self.origin_network as u64));

        // Origin token address (5 u32 felts)
        elements.extend(self.origin_token_address.to_elements());

        // Destination network
        elements.push(Felt::new(self.destination_network as u64));

        // Destination address (5 u32 felts)
        elements.extend(self.destination_address.to_elements());

        // Amount (uint256 as 8 u32 felts)
        elements.extend(self.amount.as_array().iter().map(|&v| Felt::new(v as u64)));

        // Metadata (8 u32 felts)
        elements.extend(self.metadata.iter().map(|&v| Felt::new(v as u64)));

        elements
    }
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
        // Total elements:
        //   32 + 32 (proofs, now fixed size)
        // + 8 (global index)
        // + 8 + 8 (exit roots)
        // + 1 + 5 + 1 + 5 (networks + addresses)
        // + 8 + 8 (amount + metadata)
        // + 4 (padding)
        // + 4 (output serial num)
        // + 2 (target faucet account id)
        // + 1 (note tag)
        // = 127
        let mut claim_inputs = Vec::with_capacity(127);

        // 1) PROOF DATA - use the new to_elements method
        let proof_elements = inputs.proof_data.to_elements()?;
        claim_inputs.extend(proof_elements);

        // 2) LEAF DATA - use the new to_elements method
        claim_inputs.extend(inputs.leaf_data.to_elements());

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
