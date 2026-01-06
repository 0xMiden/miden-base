use alloc::string::String;
use alloc::vec::Vec;

use miden_protocol::Felt;

/// Convert 8 Felt values (u32 limbs in little-endian order) to U256 bytes in little-endian format.
///
/// The input limbs are expected to be in little-endian order (least significant limb first).
/// This function converts them to a 32-byte array in little-endian format for compatibility
/// with Ethereum/EVM which expects U256 values as 32 bytes in little-endian format.
/// This ensures compatibility when bridging assets between Miden and Ethereum-based chains.
pub fn felts_to_u256_bytes(limbs: [Felt; 8]) -> [u8; 32] {
    let mut bytes = [0u8; 32];

    for (i, limb) in limbs.iter().enumerate() {
        let u32_value = limb.as_int() as u32;
        let limb_bytes = u32_value.to_le_bytes();
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&limb_bytes);
    }

    bytes
}

/// Converts an Ethereum address (20 bytes) into a vector of 5 Felt values.
///
/// An Ethereum address is 20 bytes, which we split into 5 u32 values (4 bytes each).
/// The address bytes are distributed as follows:
/// - u32\[0\]: bytes 0-3
/// - u32\[1\]: bytes 4-7
/// - u32\[2\]: bytes 8-11
/// - u32\[3\]: bytes 12-15
/// - u32\[4\]: bytes 16-19
///
/// # Arguments
/// * `address` - A 20-byte Ethereum address
///
/// # Returns
/// A vector of 5 Felt values representing the address
///
/// # Panics
/// Panics if the address is not exactly 20 bytes
pub fn ethereum_address_to_felts(address: &[u8; 20]) -> Vec<Felt> {
    let mut result = Vec::with_capacity(5);

    // Convert each 4-byte chunk to a u32 (big-endian)
    for i in 0..5 {
        let start = i * 4;
        let chunk = &address[start..start + 4];
        let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        result.push(Felt::new(value as u64));
    }

    result
}

/// Converts a vector of 5 Felt values back into a 20-byte Ethereum address.
///
/// # Arguments
/// * `felts` - A vector of 5 Felt values representing an Ethereum address
///
/// # Returns
/// A Result containing a 20-byte Ethereum address array, or an error string
///
/// # Errors
/// Returns an error if the vector doesn't contain exactly 5 felts
pub fn felts_to_ethereum_address(felts: &[Felt]) -> Result<[u8; 20], String> {
    if felts.len() != 5 {
        return Err(alloc::format!("Expected 5 felts for Ethereum address, got {}", felts.len()));
    }

    let mut address = [0u8; 20];

    for (i, felt) in felts.iter().enumerate() {
        let value = felt.as_int() as u32;
        let bytes = value.to_be_bytes();
        let start = i * 4;
        address[start..start + 4].copy_from_slice(&bytes);
    }

    Ok(address)
}

/// Converts an Ethereum address string (with or without "0x" prefix) into a vector of 5 Felt
/// values.
///
/// # Arguments
/// * `address_str` - A hex string representing an Ethereum address (40 hex chars, optionally
///   prefixed with "0x")
///
/// # Returns
/// A Result containing a vector of 5 Felt values representing the address, or an error string
///
/// # Errors
/// Returns an error if:
/// - The string is not a valid hex string
/// - The string does not represent exactly 20 bytes (40 hex characters)
pub fn ethereum_address_string_to_felts(address_str: &str) -> Result<Vec<Felt>, String> {
    // Remove "0x" prefix if present
    let hex_str = address_str.strip_prefix("0x").unwrap_or(address_str);

    // Check length (should be 40 hex characters for 20 bytes)
    if hex_str.len() != 40 {
        return Err(alloc::format!(
            "Invalid Ethereum address length: expected 40 hex characters, got {}",
            hex_str.len()
        ));
    }

    // Parse hex string to bytes
    let mut address_bytes = [0u8; 20];
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        let hex_byte = core::str::from_utf8(chunk)
            .map_err(|_| String::from("Invalid UTF-8 in address string"))?;
        address_bytes[i] = u8::from_str_radix(hex_byte, 16)
            .map_err(|_| alloc::format!("Invalid hex character in address: {}", hex_byte))?;
    }

    Ok(ethereum_address_to_felts(&address_bytes))
}

/// Converts a bytes32 value (32 bytes) into a vector of 8 Felt values.
///
/// A bytes32 value is 32 bytes, which we split into 8 u32 values (4 bytes each).
/// The bytes are distributed as follows:
/// - u32\[0\]: bytes 0-3
/// - u32\[1\]: bytes 4-7
/// - u32\[2\]: bytes 8-11
/// - u32\[3\]: bytes 12-15
/// - u32\[4\]: bytes 16-19
/// - u32\[5\]: bytes 20-23
/// - u32\[6\]: bytes 24-27
/// - u32\[7\]: bytes 28-31
///
/// # Arguments
/// * `bytes32` - A 32-byte value (e.g., hash, root)
///
/// # Returns
/// A vector of 8 Felt values representing the bytes32 value
///
/// # Panics
/// Panics if the input is not exactly 32 bytes
pub fn bytes32_to_felts(bytes32: &[u8; 32]) -> Vec<Felt> {
    let mut result = Vec::with_capacity(8);

    // Convert each 4-byte chunk to a u32 (big-endian)
    for i in 0..8 {
        let start = i * 4;
        let chunk = &bytes32[start..start + 4];
        let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        result.push(Felt::new(value as u64));
    }

    result
}

/// Converts a vector of 8 Felt values back into a 32-byte array.
///
/// # Arguments
/// * `felts` - A vector of 8 Felt values representing a bytes32 value
///
/// # Returns
/// A Result containing a 32-byte array, or an error string
///
/// # Errors
/// Returns an error if the vector doesn't contain exactly 8 felts
pub fn felts_to_bytes32(felts: &[Felt]) -> Result<[u8; 32], String> {
    if felts.len() != 8 {
        return Err(alloc::format!("Expected 8 felts for bytes32, got {}", felts.len()));
    }

    let mut bytes32 = [0u8; 32];

    for (i, felt) in felts.iter().enumerate() {
        let value = felt.as_int() as u32;
        let bytes = value.to_be_bytes();
        let start = i * 4;
        bytes32[start..start + 4].copy_from_slice(&bytes);
    }

    Ok(bytes32)
}

/// Converts a bytes32 hex string (with or without "0x" prefix) into a vector of 8 Felt values.
///
/// # Arguments
/// * `bytes32_str` - A hex string representing a bytes32 value (64 hex chars, optionally prefixed
///   with "0x")
///
/// # Returns
/// A Result containing a vector of 8 Felt values representing the bytes32 value, or an error string
///
/// # Errors
/// Returns an error if:
/// - The string is not a valid hex string
/// - The string does not represent exactly 32 bytes (64 hex characters)
pub fn bytes32_string_to_felts(bytes32_str: &str) -> Result<Vec<Felt>, String> {
    // Remove "0x" prefix if present
    let hex_str = bytes32_str.strip_prefix("0x").unwrap_or(bytes32_str);

    // Check length (should be 64 hex characters for 32 bytes)
    if hex_str.len() != 64 {
        return Err(alloc::format!(
            "Invalid bytes32 length: expected 64 hex characters, got {}",
            hex_str.len()
        ));
    }

    // Parse hex string to bytes
    let mut bytes32_array = [0u8; 32];
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        let hex_byte = core::str::from_utf8(chunk)
            .map_err(|_| String::from("Invalid UTF-8 in bytes32 string"))?;
        bytes32_array[i] = u8::from_str_radix(hex_byte, 16)
            .map_err(|_| alloc::format!("Invalid hex character in bytes32: {}", hex_byte))?;
    }

    Ok(bytes32_to_felts(&bytes32_array))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_ethereum_address_round_trip() {
        // Test that converting from string to felts and back gives the same result
        let original_address = "0x1234567890abcdef1122334455667788990011aa";

        // Convert string to felts
        let felts = ethereum_address_string_to_felts(original_address).unwrap();

        // Convert felts back to bytes
        let recovered_bytes = felts_to_ethereum_address(&felts).unwrap();

        // Convert original string to bytes for comparison
        let original_hex = original_address.strip_prefix("0x").unwrap();
        let mut expected_bytes = [0u8; 20];
        for (i, chunk) in original_hex.as_bytes().chunks(2).enumerate() {
            let hex_byte = core::str::from_utf8(chunk).unwrap();
            expected_bytes[i] = u8::from_str_radix(hex_byte, 16).unwrap();
        }

        // Assert they match
        assert_eq!(recovered_bytes, expected_bytes);
    }

    #[test]
    fn test_ethereum_address_to_felts_basic() {
        let address: [u8; 20] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        ];

        let result = ethereum_address_to_felts(&address);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], Felt::new(0x12345678));
        assert_eq!(result[1], Felt::new(0x9abcdef0));
    }

    #[test]
    fn test_felts_to_ethereum_address_invalid_length() {
        let felts = vec![Felt::new(1), Felt::new(2)]; // Only 2 felts
        let result = felts_to_ethereum_address(&felts);
        assert!(result.is_err());
    }

    #[test]
    fn test_ethereum_address_string_invalid_length() {
        let address_str = "0x123456"; // Too short
        let result = ethereum_address_string_to_felts(address_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_bytes32_round_trip() {
        // Test that converting from string to felts and back gives the same result
        let original_bytes32 = "0x1234567890abcdef1122334455667788990011aabbccddeeff00112233445566";

        // Convert string to felts
        let felts = bytes32_string_to_felts(original_bytes32).unwrap();

        // Convert felts back to bytes
        let recovered_bytes = felts_to_bytes32(&felts).unwrap();

        // Convert original string to bytes for comparison
        let original_hex = original_bytes32.strip_prefix("0x").unwrap();
        let mut expected_bytes = [0u8; 32];
        for (i, chunk) in original_hex.as_bytes().chunks(2).enumerate() {
            let hex_byte = core::str::from_utf8(chunk).unwrap();
            expected_bytes[i] = u8::from_str_radix(hex_byte, 16).unwrap();
        }

        // Assert they match
        assert_eq!(recovered_bytes, expected_bytes);
    }

    #[test]
    fn test_bytes32_to_felts_basic() {
        let bytes32: [u8; 32] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x66, 0x77, 0x88,
        ];

        let result = bytes32_to_felts(&bytes32);
        assert_eq!(result.len(), 8);
        assert_eq!(result[0], Felt::new(0x12345678));
        assert_eq!(result[1], Felt::new(0x9abcdef0));
    }

    #[test]
    fn test_felts_to_bytes32_invalid_length() {
        let felts = vec![Felt::new(1), Felt::new(2)]; // Only 2 felts
        let result = felts_to_bytes32(&felts);
        assert!(result.is_err());
    }

    #[test]
    fn test_bytes32_string_invalid_length() {
        let bytes32_str = "0x123456"; // Too short
        let result = bytes32_string_to_felts(bytes32_str);
        assert!(result.is_err());
    }
}
