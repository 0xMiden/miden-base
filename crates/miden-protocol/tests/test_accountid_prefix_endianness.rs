//! Test to verify AccountIdPrefix serialization endianness consistency

use miden_protocol::account::AccountIdPrefix;
use miden_protocol::utils::serde::{Deserializable, Serializable};

#[test]
fn test_accountid_prefix_endianness_roundtrip() {
    // Create a test AccountIdPrefix from known bytes
    let bytes: [u8; 8] = [170, 0, 0, 0, 0, 0, 188, 32];

    println!("Original bytes: {:?}", bytes);

    // Deserialize
    let prefix = match AccountIdPrefix::read_from_bytes(&bytes) {
        Ok(p) => {
            println!("✓ Deserialization succeeded: {:?}", p);
            p
        },
        Err(e) => {
            println!("✗ Deserialization failed: {:?}", e);
            panic!("Failed to deserialize AccountIdPrefix");
        },
    };

    // Serialize back
    let serialized = prefix.to_bytes();
    println!("Serialized bytes: {:?}", serialized);

    // Verify roundtrip
    assert_eq!(
        bytes,
        serialized.as_slice(),
        "Roundtrip failed: serialized bytes don't match original"
    );
}

#[test]
fn test_accountid_prefix_version_extraction() {
    // Test that version byte is extracted correctly from various prefixes
    let test_cases = vec![
        ([170, 0, 0, 0, 0, 0, 188, 32], "Faucet ID"),
        ([188, 0, 0, 0, 0, 0, 202, 48], "Non-fungible faucet"),
    ];

    for (bytes, description) in test_cases {
        println!("\nTesting {}: {:?}", description, bytes);

        match AccountIdPrefix::read_from_bytes(&bytes) {
            Ok(prefix) => {
                let version = prefix.version();
                println!("✓ Version extracted: {:?}", version);

                // Version should always be 0 for V0
                assert_eq!(
                    format!("{:?}", version),
                    "Version0",
                    "Expected Version0 for {}",
                    description
                );
            },
            Err(e) => {
                println!("✗ Failed to deserialize {}: {:?}", description, e);
                panic!("Version extraction failed for {}", description);
            },
        }
    }
}
