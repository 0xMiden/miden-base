use miden_core::FieldElement;
use miden_protocol::Felt;

// UTILITY FUNCTIONS
// ================================================================================================

/// Converts a bytes32 value (32 bytes) into an array of 8 Felt values.
pub fn bytes32_to_felts(bytes32: &[u8; 32]) -> [Felt; 8] {
    let mut result = [Felt::ZERO; 8];
    for (i, chunk) in bytes32.chunks(4).enumerate() {
        let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        result[i] = Felt::new(value as u64);
    }
    result
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
