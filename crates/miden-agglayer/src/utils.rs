use alloc::vec::Vec;

use miden_protocol::Felt;

// UTILITY FUNCTIONS
// ================================================================================================

/// Converts a bytes32 value (32 bytes) into a vector of 8 Felt values.
pub fn bytes32_to_felts(bytes32: &[u8; 32]) -> Vec<Felt> {
    bytes32
        .chunks(4)
        .map(|chunk| {
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            Felt::new(value as u64)
        })
        .collect()
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
