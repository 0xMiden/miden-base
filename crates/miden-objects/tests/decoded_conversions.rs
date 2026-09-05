use miden_objects::{DecodeMessage, proto};
use miden_protocol::Word;

#[test]
fn word_atomic_decode() {
    assert_eq!(
        proto::primitives::Word::from(Word::empty()).decode_fields().unwrap(),
        Word::empty()
    );
}

#[test]
fn felt_atomic_decode() {
    assert!(
        proto::primitives::Felt { value: miden_protocol::Felt::ORDER }
            .decode_fields()
            .is_err()
    );
}

#[test]
fn mast_forest_atomic_decode() {
    let mast = miden_protocol::MastForest::new();
    assert_eq!(proto::primitives::MastForest::from(&mast).decode_fields().unwrap(), mast);
}

#[test]
fn execution_proof_atomic_decode() {
    let proof = miden_protocol::testing::dummy_execution_proof();
    assert_eq!(proto::primitives::ExecutionProof::from(&proof).decode_fields().unwrap(), proof);
}
