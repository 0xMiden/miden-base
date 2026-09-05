use miden_objects::{DecodeMessage, Verify, proto};
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

#[test]
fn account_id_atomic_decode() {
    assert!(proto::account::AccountId { id: vec![] }.decode_fields().is_err());
}

#[test]
fn kernel_verification_is_deferred() {
    let decoded = proto::protocol_config::KernelConfig {
        main_proc: Some(Word::empty().into()),
        kernel_procs: vec![
            Word::empty().into();
            miden_protocol::protocol_config::KernelConfig::MAX_NUM_KERNEL_PROCEDURES
                + 1
        ],
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn security_policy_verification_is_deferred() {
    for minimum_bits in [0, u32::MAX] {
        let decoded = proto::protocol_config::ProofSecurityPolicy {
            security_estimator_root: Some(Word::empty().into()),
            minimum_bits,
        }
        .decode_fields()
        .unwrap();
        assert_eq!(decoded.minimum_bits, minimum_bits);
        assert!(decoded.verify().is_err());
    }
}

#[test]
fn proof_verification_config_defers_nested_policy_checks() {
    let decoded = proto::protocol_config::ProofVerificationConfig {
        vm_verifier_root: Some(Word::empty().into()),
        precompile_verifier_root: Some(Word::empty().into()),
        security_policy: Some(proto::protocol_config::ProofSecurityPolicy {
            security_estimator_root: Some(Word::empty().into()),
            minimum_bits: 0,
        }),
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn merkle_path_verifies() {
    let decoded = proto::primitives::MerklePath { siblings: vec![Word::empty().into()] }
        .decode_fields()
        .unwrap();
    assert_eq!(decoded.verify().unwrap().nodes(), &[Word::empty()]);
}

#[test]
fn sparse_path_defers_depth_validation() {
    let decoded = proto::primitives::SparseMerklePath {
        empty_nodes_mask: 0,
        siblings: vec![Word::empty().into(); 65],
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn smt_leaf_entry_verifies() {
    let decoded = proto::primitives::SmtLeafEntry {
        key: Some(Word::empty().into()),
        value: Some(Word::empty().into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap(), (Word::empty(), Word::empty()));
}

#[test]
fn storage_slot_id_verifies() {
    let decoded = proto::account::StorageSlotId {
        suffix: Some(miden_protocol::Felt::ONE.into()),
        prefix: Some(miden_protocol::Felt::ZERO.into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap().suffix(), miden_protocol::Felt::ONE);
}

#[test]
fn asset_class_verifies() {
    let decoded = proto::asset::AssetClass {
        suffix: Some(miden_protocol::Felt::ONE.into()),
        prefix: Some(miden_protocol::Felt::ZERO.into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap().suffix(), miden_protocol::Felt::ONE);
}

#[test]
fn transaction_id_verifies() {
    let decoded = proto::transaction::TransactionId { id: Some(Word::empty().into()) }
        .decode_fields()
        .unwrap();
    assert_eq!(
        decoded.verify().unwrap(),
        miden_protocol::transaction::TransactionId::from_raw(Word::empty())
    );
}

#[test]
fn partial_smt_node_verifies() {
    assert_eq!(
        proto::primitives::PartialSmtNode {
            index: 7,
            digest: Some(Word::empty().into())
        }
        .decode_fields()
        .unwrap()
        .verify()
        .unwrap(),
        (7, Word::empty())
    );
}

#[test]
fn partial_smt_level_verifies_nested_nodes() {
    let decoded = proto::primitives::PartialSmtNodeLevel {
        depth: 2,
        nodes: vec![proto::primitives::PartialSmtNode {
            index: 3,
            digest: Some(Word::empty().into()),
        }],
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap(), (2, vec![(3, Word::empty())]));
}

#[test]
fn indexed_digest_verifies() {
    assert_eq!(
        proto::primitives::IndexedDigest {
            index: 5,
            value: Some(Word::empty().into())
        }
        .decode_fields()
        .unwrap()
        .verify()
        .unwrap(),
        (5, Word::empty())
    );
}

#[test]
fn smt_entry_list_verifies() {
    assert!(
        proto::primitives::SmtLeafEntryList { entries: vec![] }
            .decode_fields()
            .unwrap()
            .verify()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn merkle_store_node_verifies() {
    let decoded = proto::primitives::MerkleStoreNode {
        value: Some(Word::empty().into()),
        left: Some(Word::empty().into()),
        right: Some(Word::empty().into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap().value, Word::empty());
}

#[test]
fn advice_map_entry_verifies() {
    let decoded = proto::primitives::AdviceMapEntry {
        key: Some(Word::empty().into()),
        values: vec![miden_protocol::Felt::ONE.into()],
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap(), (Word::empty(), vec![miden_protocol::Felt::ONE]));
}

#[test]
fn storage_map_entry_verifies() {
    let decoded = proto::account::StorageMapEntry {
        key: Some(Word::empty().into()),
        value: Some(Word::empty().into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap().1, Word::empty());
}

#[test]
fn tracked_mmr_leaf_verifies() {
    let decoded = proto::blockchain::TrackedMmrLeaf {
        position: 2,
        leaf: Some(Word::empty().into()),
        path: vec![],
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap(), (2, Word::empty(), vec![]));
}

#[test]
fn block_number_verifies() {
    assert_eq!(
        proto::blockchain::BlockNumber { block_num: u32::MAX }
            .decode_fields()
            .unwrap()
            .verify()
            .unwrap()
            .as_u32(),
        u32::MAX
    );
}

#[test]
fn fee_parameters_verify() {
    assert_eq!(
        proto::blockchain::FeeParameters { verification_base_fee: 7 }
            .decode_fields()
            .unwrap()
            .verify()
            .unwrap()
            .verification_base_fee(),
        7
    );
}

#[test]
fn advice_stack_verifies_in_order() {
    let decoded = proto::primitives::AdviceStack {
        values: vec![miden_protocol::Felt::ONE.into(), miden_protocol::Felt::ZERO.into()],
    }
    .decode_fields()
    .unwrap();
    assert_eq!(
        decoded.verify().unwrap().iter().copied().collect::<Vec<_>>(),
        vec![miden_protocol::Felt::ONE, miden_protocol::Felt::ZERO]
    );
}

#[test]
fn note_id_verifies() {
    let decoded = proto::note::NoteId { id: Some(Word::empty().into()) }.decode_fields().unwrap();
    assert_eq!(decoded.verify().unwrap(), miden_protocol::note::NoteId::from_raw(Word::empty()));
}

#[test]
fn account_code_defers_procedure_validation() {
    let decoded = proto::account::AccountCode {
        mast: Some(miden_protocol::MastForest::new().into()),
        procedure_roots: vec![],
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}
