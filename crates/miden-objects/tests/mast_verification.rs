use miden_objects::{DecodeMessage, Verify, decoded, proto};
use miden_protocol::assembly::mast::MastForestError;
use miden_protocol::utils::serde::Serializable;
use miden_protocol::{MastForest, Word};

fn corrupt_node_hash(forest: &MastForest, digest: Word) -> proto::primitives::MastForest {
    let mut wire = proto::primitives::MastForest::from(forest);
    let digest = digest.to_bytes();
    let offsets: Vec<_> = wire
        .encoded
        .windows(digest.len())
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == digest).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1, "the fixture must contain the node digest exactly once");
    let replacement = Word::from([9_u32, 8, 7, 6]).to_bytes();
    assert_ne!(replacement, digest);
    wire.encoded[offsets[0]..offsets[0] + digest.len()].copy_from_slice(&replacement);
    wire
}

#[test]
fn corrupted_node_hash_is_rejected_during_verification() {
    let script = miden_protocol::note::NoteScript::mock();
    let wire = corrupt_node_hash(&script.mast(), script.root().into());
    assert!(matches!(
        wire.decode_fields().unwrap().verify(),
        Err(MastForestError::HashMismatch { .. })
    ));
}

#[test]
fn note_script_validates_its_forest() {
    let script = miden_protocol::note::NoteScript::mock();
    let mast = corrupt_node_hash(&script.mast(), script.root().into());
    let wire = proto::note::NoteScript {
        mast: Some(mast),
        entrypoint: script.entrypoint().into(),
    };
    assert!(matches!(
        wire.decode_fields().unwrap().verify(),
        Err(decoded::note::VerificationError::Mast(MastForestError::HashMismatch { .. }))
    ));
}

#[test]
fn transaction_script_validates_its_forest() {
    let script = miden_protocol::note::NoteScript::mock();
    let mast = corrupt_node_hash(&script.mast(), script.root().into());
    let wire = proto::transaction::TransactionScript {
        mast: Some(mast),
        entrypoint: script.entrypoint().into(),
    };
    assert!(matches!(
        wire.decode_fields().unwrap().verify(),
        Err(decoded::transaction::ScriptError::Mast(MastForestError::HashMismatch { .. }))
    ));
}

#[test]
fn account_code_validates_its_forest() {
    let code = miden_protocol::account::AccountCode::mock();
    let mast = corrupt_node_hash(&code.mast(), code.procedure_roots().next().unwrap());
    let mut wire = proto::account::AccountCode::from(&code);
    wire.mast = Some(mast);
    assert!(matches!(
        wire.decode_fields().unwrap().verify(),
        Err(decoded::account::AccountCodeError::Mast(MastForestError::HashMismatch { .. }))
    ));
}

#[test]
fn malformed_mast_bytes_have_generated_paths() {
    let script = miden_protocol::note::NoteScript::mock();
    let mut trailing = proto::primitives::MastForest::from(script.mast().as_ref());
    trailing.encoded.push(0);
    for mast in [proto::primitives::MastForest { encoded: vec![] }, trailing] {
        let error = mast.clone().decode_fields().unwrap_err();
        assert!(error.to_string().starts_with("encoded: "), "{error}");
        let wire = proto::note::NoteScript {
            mast: Some(mast),
            entrypoint: script.entrypoint().into(),
        };
        let error = wire.decode_fields().unwrap_err();
        assert!(error.to_string().starts_with("mast.encoded: "), "{error}");
    }
}
