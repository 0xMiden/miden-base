use alloc::string::String;
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
use crate::utils::bytes32_to_felts;

// CLAIM NOTE STRUCTURES
// ================================================================================================

/// Proof data for CLAIM note creation.
/// Contains SMT proofs and root hashes using native types.
pub struct ProofData<'a> {
    /// SMT proof for local exit root (bytes32[_DEPOSIT_CONTRACT_TREE_DEPTH])
    /// 256 elements, each bytes32 represented as 32-byte array
    pub smt_proof_local_exit_root: &'a [[u8; 32]],
    /// SMT proof for rollup exit root (bytes32[_DEPOSIT_CONTRACT_TREE_DEPTH])
    /// 256 elements, each bytes32 represented as 32-byte array
    pub smt_proof_rollup_exit_root: &'a [[u8; 32]],
    /// Global index (uint256 as 8 u32 values)
    pub global_index: [u32; 8],
    /// Mainnet exit root hash (bytes32 as 32-byte array)
    pub mainnet_exit_root: &'a [u8; 32],
    /// Rollup exit root hash (bytes32 as 32-byte array)
    pub rollup_exit_root: &'a [u8; 32],
}

/// Leaf data for CLAIM note creation.
/// Contains network, address, amount, and metadata using native types.
pub struct LeafData<'a> {
    /// Origin network identifier (uint32)
    pub origin_network: u32,
    /// Origin token address (address as 20-byte array)
    pub origin_token_address: &'a [u8; 20],
    /// Destination network identifier (uint32)
    pub destination_network: u32,
    /// Destination address (address as 20-byte array)
    pub destination_address: &'a [u8; 20],
    /// Amount of tokens (uint256 as 8 u32 values)
    pub amount: [u32; 8],
    /// ABI encoded metadata (fixed size of 8 u32 values)
    pub metadata: [u32; 8],
}

/// Output note data for CLAIM note creation.
/// Contains note-specific data and can use Miden types.
pub struct OutputNoteData {
    /// P2ID note serial number (4 felts as Word)
    pub output_p2id_serial_num: Word,
    /// Target faucet account ID (2 felts: prefix and suffix)
    pub target_faucet_account_id: AccountId,
    /// P2ID output note tag
    pub output_note_tag: NoteTag,
}

/// Parameters for creating a CLAIM note.
///
/// This struct groups all the parameters needed to create a CLAIM note that exactly
/// matches the agglayer claimAsset function signature.
pub struct ClaimNoteParams<'a, R: FeltRng> {
    /// Proof data containing SMT proofs and root hashes
    pub proof_data: ProofData<'a>,
    /// Leaf data containing network, address, amount, and metadata
    pub leaf_data: LeafData<'a>,
    /// Output note data containing note-specific information
    pub output_note_data: OutputNoteData,
    /// CLAIM note sender account id
    pub claim_note_creator_account_id: AccountId,
    /// TODO: remove and use destination_address: [u8; 20]
    pub destination_account_id: AccountId,
    /// RNG for creating CLAIM note serial number
    pub rng: &'a mut R,
}

// CLAIM NOTE CREATION
// ================================================================================================

/// Generates a CLAIM note - a note that instructs an agglayer faucet to validate and mint assets.
///
/// # Parameters
/// - `params`: The parameters for creating the CLAIM note (including RNG)
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_claim_note<R: FeltRng>(params: ClaimNoteParams<'_, R>) -> Result<Note, NoteError> {
    // Validate SMT proof lengths - each should be 256 elements (bytes32 values)
    ensure_len(
        params.proof_data.smt_proof_local_exit_root.len(),
        256,
        "smt_proof_local_exit_root",
    )?;
    ensure_len(
        params.proof_data.smt_proof_rollup_exit_root.len(),
        256,
        "smt_proof_rollup_exit_root",
    )?;

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
    push_smt_proof_u32_strict(&mut claim_inputs, params.proof_data.smt_proof_local_exit_root)?;
    push_smt_proof_u32_strict(&mut claim_inputs, params.proof_data.smt_proof_rollup_exit_root)?;

    // globalIndex (uint256 as 8 u32 felts)
    push_u32_words(&mut claim_inputs, &params.proof_data.global_index);

    // mainnetExitRoot (bytes32 as 8 u32 felts)
    claim_inputs.extend(bytes32_to_felts(params.proof_data.mainnet_exit_root));

    // rollupExitRoot (bytes32 as 8 u32 felts)
    claim_inputs.extend(bytes32_to_felts(params.proof_data.rollup_exit_root));

    // 2) LEAF DATA
    claim_inputs.push(Felt::new(params.leaf_data.origin_network as u64));

    // originTokenAddress (address as 5 u32 felts)
    claim_inputs
        .extend(crate::EthAddressFormat::new(*params.leaf_data.origin_token_address).to_elements());

    claim_inputs.push(Felt::new(params.leaf_data.destination_network as u64));

    // destinationAddress (address as 5 u32 felts)
    claim_inputs
        .extend(crate::EthAddressFormat::new(*params.leaf_data.destination_address).to_elements());

    // amount (uint256 as 8 u32 felts)
    push_u32_words(&mut claim_inputs, &params.leaf_data.amount);

    // metadata (fixed size of 8 felts)
    push_u32_words(&mut claim_inputs, &params.leaf_data.metadata);

    // Keep the same trailing padding as the original implementation.
    claim_inputs.extend(core::iter::repeat_n(Felt::ZERO, 4));

    // 3) OUTPUT NOTE DATA
    claim_inputs.extend(params.output_note_data.output_p2id_serial_num);

    // target_faucet_account_id (2 felts: prefix and suffix)
    claim_inputs.push(params.output_note_data.target_faucet_account_id.prefix().as_felt());
    claim_inputs.push(params.output_note_data.target_faucet_account_id.suffix());

    // output note tag
    claim_inputs.push(Felt::new(params.output_note_data.output_note_tag.as_u32() as u64));

    let inputs = NoteInputs::new(claim_inputs)?;

    // Use a default tag since we don't have agg_faucet_id anymore.
    let tag = NoteTag::for_local_use_case(0, 0)?;

    let metadata = NoteMetadata::new(
        params.claim_note_creator_account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::ZERO,
    )?;

    let recipient = NoteRecipient::new(params.rng.draw_word(), claim_script(), inputs);
    let assets = NoteAssets::new(vec![])?;

    Ok(Note::new(assets, metadata, recipient))
}

fn ensure_len(actual: usize, expected: usize, name: &str) -> Result<(), NoteError> {
    if actual == expected {
        Ok(())
    } else {
        Err(NoteError::other(alloc::format!(
            "{name} must be exactly {expected} elements, got {actual}"
        )))
    }
}

fn push_smt_proof_u32_strict(out: &mut Vec<Felt>, proof: &[[u8; 32]]) -> Result<(), NoteError> {
    for (i, b) in proof.iter().enumerate() {
        let felt = bytes32_to_u32_felt_strict(b).map_err(|e| {
            NoteError::other(alloc::format!("invalid SMT proof element at index {i}: {e}"))
        })?;
        out.push(felt);
    }
    Ok(())
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

fn push_u32_words(out: &mut Vec<Felt>, words: &[u32]) {
    out.extend(words.iter().map(|&v| Felt::new(v as u64)));
}

// SOLIDITY TYPE CONVERSION FUNCTIONS
// ================================================================================================

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(alloc::format!("invalid hex character: {}", b as char)),
    }
}

fn decode_fixed_hex<const N: usize>(hex_str: &str) -> Result<[u8; N], String> {
    let s = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let expected_len = N * 2;

    if s.len() != expected_len {
        return Err(alloc::format!(
            "invalid hex string length: expected {} chars, got {}",
            expected_len,
            s.len()
        ));
    }

    let mut out = [0u8; N];
    let bytes = s.as_bytes();

    for i in 0..N {
        let hi = hex_nibble(bytes[2 * i])?;
        let lo = hex_nibble(bytes[2 * i + 1])?;
        out[i] = (hi << 4) | lo;
    }

    Ok(out)
}

/// Converts a hex string (with or without 0x prefix) to a bytes32 array.
///
/// # Parameters
/// - `hex_str`: Hex string representation of bytes32 (64 hex chars + optional 0x prefix)
///
/// # Returns
/// A 32-byte array representing the bytes32 value
///
/// # Errors
/// Returns an error if the hex string is invalid or not the correct length
pub fn hex_string_to_bytes32(hex_str: &str) -> Result<[u8; 32], String> {
    decode_fixed_hex::<32>(hex_str)
}

/// Converts a hex string (with or without 0x prefix) to an Ethereum address (20 bytes).
///
/// # Parameters
/// - `hex_str`: Hex string representation of address (40 hex chars + optional 0x prefix)
///
/// # Returns
/// A 20-byte array representing the Ethereum address
///
/// # Errors
/// Returns an error if the hex string is invalid or not the correct length
pub fn hex_string_to_address(hex_str: &str) -> Result<[u8; 20], String> {
    decode_fixed_hex::<20>(hex_str)
}

/// Converts a u256 value (as a decimal string or hex string) to a u32 array.
///
/// # Parameters
/// - `value_str`: String representation of uint256 (decimal or hex with 0x prefix)
///
/// # Returns
/// An 8-element u32 array representing the uint256 value (little-endian)
///
/// # Errors
/// Returns an error if the string cannot be parsed as a valid number
pub fn string_to_u256_array(value_str: &str) -> Result<[u32; 8], String> {
    let mut result = [0u32; 8];

    if let Some(hex_str) = value_str.strip_prefix("0x") {
        // Parse as hex
        if hex_str.len() > 64 {
            return Err(alloc::format!("hex string too long for u256: {}", hex_str.len()));
        }

        // Left-pad with leading zeros up to 32 bytes.
        let mut buf = [b'0'; 64];
        let src = hex_str.as_bytes();
        buf[64 - src.len()..].copy_from_slice(src);

        // Parse 8 u32 values from the hex string (big-endian in string, but we store little-endian)
        for i in 0..8 {
            let start = i * 8;
            let chunk = core::str::from_utf8(&buf[start..start + 8])
                .map_err(|_| String::from("invalid UTF-8 in hex string"))?;
            let value = u32::from_str_radix(chunk, 16)
                .map_err(|_| alloc::format!("invalid hex chunk: {chunk}"))?;
            result[7 - i] = value;
        }

        return Ok(result);
    }

    // Decimal parsing: supports up to u128 here; larger values should be provided as hex.
    let value: u128 = value_str
        .parse()
        .map_err(|_| alloc::format!("invalid decimal number: {}", value_str))?;

    for (i, item) in result.iter_mut().enumerate().take(4) {
        *item = (value >> (i * 32)) as u32;
    }

    Ok(result)
}

/// Converts an array of hex strings to an array of bytes32 arrays.
///
/// # Parameters
/// - `hex_strings`: Slice of hex string references (each should be 64 hex chars + optional 0x
///   prefix)
///
/// # Returns
/// A vector of 32-byte arrays
///
/// # Errors
/// Returns an error if any hex string is invalid
pub fn hex_strings_to_bytes32_array(hex_strings: &[&str]) -> Result<Vec<[u8; 32]>, String> {
    let mut result = Vec::with_capacity(hex_strings.len());

    for (i, hex_str) in hex_strings.iter().enumerate() {
        let bytes32 = hex_string_to_bytes32(hex_str)
            .map_err(|e| alloc::format!("error parsing hex string at index {}: {}", i, e))?;
        result.push(bytes32);
    }

    Ok(result)
}

/// Converts metadata bytes (hex string or raw bytes) to a u32 array of fixed size 8.
///
/// # Parameters
/// - `metadata_hex`: Hex string representation of metadata (optional 0x prefix)
///
/// # Returns
/// An 8-element u32 array representing the metadata
///
/// # Errors
/// Returns an error if the hex string is invalid
pub fn metadata_hex_to_u32_array(metadata_hex: &str) -> Result<[u32; 8], String> {
    let s = metadata_hex.strip_prefix("0x").unwrap_or(metadata_hex);

    // Pad (right) or truncate to 64 hex chars (32 bytes = 8 u32 values)
    let mut buf = [b'0'; 64];
    let src = s.as_bytes();
    let n = core::cmp::min(src.len(), 64);
    buf[..n].copy_from_slice(&src[..n]);

    let mut result = [0u32; 8];
    for (i, item) in result.iter_mut().enumerate() {
        let start = i * 8;
        let chunk = core::str::from_utf8(&buf[start..start + 8])
            .map_err(|_| String::from("invalid UTF-8 in metadata hex"))?;
        *item = u32::from_str_radix(chunk, 16)
            .map_err(|_| alloc::format!("invalid hex chunk in metadata: {}", chunk))?;
    }

    Ok(result)
}
