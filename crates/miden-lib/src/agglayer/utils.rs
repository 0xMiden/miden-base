use alloc::vec::Vec;

use miden_objects::Felt;

/// Convert 8 Felt values (provided as 8 u32 limbs in big-endian order, most significant limb first)
/// to U256 bytes in little-endian format.
pub fn felts_to_u256_bytes(limbs: Vec<Felt>) -> Vec<u8> {
    assert_eq!(limbs.len(), 8, "Expected exactly 8 u32 limbs for U256 conversion");

    let mut bytes = Vec::with_capacity(32);
    for i in 0..8 {
        let u32_value = limbs[7 - i].as_int() as u32;
        bytes.extend_from_slice(&u32_value.to_le_bytes());
    }

    bytes
}

/// Convert Felt values to u32 values.
pub fn felts_to_u32_slice(felts: &[Felt]) -> Vec<u32> {
    felts.iter().map(|f| f.as_int() as u32).collect()
}
