use std::error::Error;

use miden_objects::{DecodeMessage, proto};
use miden_protocol::utils::serde::DeserializationError;
use miden_protocol::{Felt, Word};

#[test]
fn invalid_word_lengths_and_contents_use_the_same_generated_path() {
    for encoded in [vec![0; 31], vec![0; 33], vec![255; 32]] {
        let word = proto::primitives::Word { encoded };
        let borrowed_error = Word::try_from(&word).unwrap_err();
        let error = word.clone().decode_fields().unwrap_err();
        assert_eq!(borrowed_error.to_string(), error.to_string());
        assert!(error.to_string().starts_with("encoded: "), "{error}");
        assert!(error.source().unwrap().is::<DeserializationError>());
    }
}

#[test]
fn felt_paths_include_only_schema_fields() {
    let felt = proto::primitives::Felt { value: Felt::ORDER };
    let borrowed_error = Felt::try_from(&felt).unwrap_err();
    let error = felt.decode_fields().unwrap_err();
    assert_eq!(borrowed_error.to_string(), error.to_string());
    assert!(error.to_string().starts_with("value: "), "{error}");
    assert!(error.source().unwrap().is::<<Felt as TryFrom<u64>>::Error>());
}

#[test]
fn proof_paths_preserve_the_deserialization_source() {
    use miden_protocol::vm::ExecutionProof;

    let proof = proto::primitives::ExecutionProof { encoded: vec![] };
    let borrowed_error = ExecutionProof::try_from(&proof).unwrap_err();
    let error = proof.decode_fields().unwrap_err();
    assert_eq!(borrowed_error.to_string(), error.to_string());
    assert!(error.to_string().starts_with("encoded: "), "{error}");
    assert!(error.source().unwrap().is::<DeserializationError>());
}
