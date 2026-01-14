use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::{cmp, str};

use miden_core::FieldElement;
use miden_protocol::Felt;

// UTILITY FUNCTIONS
// ================================================================================================

/// Converts a bytes32 value (32 bytes) into an array of 8 Felt values.
///
/// Note: These utility functions will eventually be replaced with similar functions from miden-vm.
pub fn bytes32_to_felts(bytes32: &[u8; 32]) -> Result<[Felt; 8], &'static str> {
    let mut result = [Felt::ZERO; 8];
    for (i, chunk) in bytes32.chunks(4).enumerate() {
        let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        result[i] =
            Felt::try_from(value as u64).map_err(|_| "Failed to convert u32 value to Felt")?;
    }
    Ok(result)
}

/// Convert 8 Felt values (u32 limbs in little-endian order) to U256 bytes in little-endian format.
pub fn felts_to_u256_bytes(limbs: [Felt; 8]) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (i, limb) in limbs.iter().enumerate() {
        let u32_value = limb.as_int() as u32;
        let limb_bytes = u32_value.to_le_bytes();
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&limb_bytes);
    }
    bytes
}

// HEX PARSING UTILITIES
// ================================================================================================

/// Converts a single hex nibble character to its numeric value.
pub fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex character: {}", b as char)),
    }
}

/// Decodes a hex string to a fixed-size byte array.
///
/// # Parameters
/// - `hex_str`: Hex string (with or without "0x" prefix)
///
/// # Returns
/// A fixed-size byte array of length N
///
/// # Errors
/// Returns an error if the hex string is invalid or not the correct length
pub fn decode_fixed_hex<const N: usize>(hex_str: &str) -> Result<[u8; N], String> {
    let s = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let expected_len = N * 2;

    if s.len() != expected_len {
        return Err(format!(
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
            return Err(format!("hex string too long for u256: {}", hex_str.len()));
        }

        // Left-pad with leading zeros up to 32 bytes.
        let mut buf = [b'0'; 64];
        let src = hex_str.as_bytes();
        buf[64 - src.len()..].copy_from_slice(src);

        // Parse 8 u32 values from the hex string (big-endian in string, but we store little-endian)
        for i in 0..8 {
            let start = i * 8;
            let chunk = str::from_utf8(&buf[start..start + 8])
                .map_err(|_| String::from("invalid UTF-8 in hex string"))?;
            let value = u32::from_str_radix(chunk, 16)
                .map_err(|_| format!("invalid hex chunk: {chunk}"))?;
            result[7 - i] = value;
        }

        return Ok(result);
    }

    // Decimal parsing: supports up to u128 here; larger values should be provided as hex.
    let value: u128 = value_str
        .parse()
        .map_err(|_| format!("invalid decimal number: {}", value_str))?;

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
            .map_err(|e| format!("error parsing hex string at index {}: {}", i, e))?;
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
    let n = cmp::min(src.len(), 64);
    buf[..n].copy_from_slice(&src[..n]);

    let mut result = [0u32; 8];
    for (i, item) in result.iter_mut().enumerate() {
        let start = i * 8;
        let chunk = str::from_utf8(&buf[start..start + 8])
            .map_err(|_| String::from("invalid UTF-8 in metadata hex"))?;
        *item = u32::from_str_radix(chunk, 16)
            .map_err(|_| format!("invalid hex chunk in metadata: {}", chunk))?;
    }

    Ok(result)
}
