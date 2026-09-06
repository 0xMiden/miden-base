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
fn account_id_oneof_roundtrip() {
    use miden_protocol::account::{AccountId, AccountIdV1, AccountType, AssetCallbackFlag};
    use prost::Message;

    let id = AccountId::V1(AccountIdV1::dummy(
        [7; 15],
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    ));
    let bytes = proto::account::AccountId::from(id).encode_to_vec();
    let wire = proto::account::AccountId::decode(bytes.as_slice()).unwrap();
    let decoded = wire.decode_fields().unwrap();
    let proto::account::account_id::DecodedVersion::V1(v1) = &decoded.version;
    assert_eq!(v1.suffix, id.suffix());
    assert_eq!(v1.prefix, id.prefix().as_felt());
    assert_eq!(decoded.verify().unwrap(), id);
    assert_eq!(AccountId::try_from(wire).unwrap(), id);
}

#[test]
fn account_id_verification_is_deferred_in_parents() {
    use miden_protocol::errors::AccountIdError;

    let metadata = *miden_protocol::note::Note::mock_noop(Word::empty()).metadata();
    let mut wire = proto::note::NoteMetadata::from(metadata);
    let sender = wire.sender.as_mut().unwrap();
    let proto::account::account_id::Version::V1(v1) = sender.version.as_mut().unwrap();
    v1.prefix.as_mut().unwrap().value &= !0xf;

    let decoded = (*sender).decode_fields().unwrap();
    assert!(matches!(decoded.verify(), Err(AccountIdError::UnknownAccountIdVersion(0))));
    let decoded = wire.decode_fields().unwrap();
    assert!(matches!(
        decoded.verify(),
        Err(miden_objects::decoded::note::VerificationError::AccountId(
            AccountIdError::UnknownAccountIdVersion(0)
        ))
    ));
}

#[test]
fn account_id_v1_roundtrip() {
    use miden_protocol::account::{AccountIdV1, AccountType, AssetCallbackFlag};

    for account_type in [AccountType::Private, AccountType::Public] {
        for callbacks in [AssetCallbackFlag::Disabled, AssetCallbackFlag::Enabled] {
            let id = AccountIdV1::dummy([7; 15], account_type, callbacks);
            let decoded = proto::account::AccountIdV1::from(id).decode_fields().unwrap();
            assert_eq!(decoded.suffix, id.suffix());
            assert_eq!(decoded.prefix, id.prefix().as_felt());
            assert_eq!(decoded.verify().unwrap(), id);
            assert_eq!(AccountIdV1::try_from(proto::account::AccountIdV1::from(&id)).unwrap(), id);
        }
    }
}

#[test]
fn account_id_v1_verification_is_deferred() {
    use miden_protocol::errors::AccountIdError;

    for (suffix, prefix, expected) in [
        (0, 0, AccountIdError::UnknownAccountIdVersion(0)),
        (1, 1, AccountIdError::AccountIdSuffixLeastSignificantByteMustBeZero),
        (1 << 63, 1, AccountIdError::AccountIdSuffixMostSignificantBitMustBeZero),
    ] {
        let decoded = proto::account::AccountIdV1 {
            suffix: Some(proto::primitives::Felt { value: suffix }),
            prefix: Some(proto::primitives::Felt { value: prefix }),
        }
        .decode_fields()
        .unwrap();
        assert_eq!(decoded.verify().unwrap_err().to_string(), expected.to_string());
    }
}

#[test]
fn account_id_v1_decode_reports_field_paths() {
    for field in ["suffix", "prefix"] {
        for invalid in [None, Some(proto::primitives::Felt { value: miden_protocol::Felt::ORDER })]
        {
            let path = if invalid.is_some() {
                format!("{field}.felt.value:")
            } else {
                format!("{field}:")
            };
            let mut id = proto::account::AccountIdV1 {
                suffix: Some(proto::primitives::Felt { value: 0 }),
                prefix: Some(proto::primitives::Felt { value: 1 }),
            };
            match field {
                "suffix" => id.suffix = invalid,
                "prefix" => id.prefix = invalid,
                _ => unreachable!(),
            }
            let error = id.decode_fields().unwrap_err();
            assert!(error.to_string().starts_with(&path), "{error}");
            let error = proto::account::AccountId {
                version: Some(proto::account::account_id::Version::V1(id)),
            }
            .decode_fields()
            .unwrap_err();
            assert!(error.to_string().starts_with(&format!("version.v1.{path}")), "{error}");
        }
    }
}

#[test]
fn account_id_decode_reports_repeated_asset_paths() {
    let note = miden_protocol::note::Note::mock_noop(Word::empty());
    let (assets, _, recipient, _) = note.into_parts();
    let details = miden_protocol::note::NoteDetails::new(assets, recipient);
    let mut wire = proto::note::NoteDetails::from(&details);
    let index = wire.assets.len();
    wire.assets.push(proto::asset::Asset {
        asset_id: Some(proto::asset::AssetId {
            version: proto::asset::AssetVersion::V1 as i32,
            asset_class: Some(proto::asset::AssetClass {
                suffix: Some(proto::primitives::Felt { value: 0 }),
                prefix: Some(proto::primitives::Felt { value: 0 }),
            }),
            composition: proto::asset::AssetComposition::Fungible as i32,
            faucet_id: Some(proto::account::AccountId {
                version: Some(proto::account::account_id::Version::V1(
                    proto::account::AccountIdV1 {
                        suffix: Some(proto::primitives::Felt { value: 0 }),
                        prefix: None,
                    },
                )),
            }),
        }),
        value: Some(Word::empty().into()),
    });
    let error = wire.decode_fields().unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with(&format!("assets[{index}].asset_id.faucet_id.version.v1.prefix: ")),
        "{error}"
    );
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

#[test]
fn note_storage_defers_length_validation() {
    let decoded = proto::note::NoteStorage {
        items: vec![miden_protocol::Felt::ZERO.into(); miden_protocol::MAX_NOTE_STORAGE_ITEMS + 1],
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn attachment_defers_scheme_validation() {
    let decoded = proto::note::NoteAttachment { scheme: u32::MAX, words: vec![] }
        .decode_fields()
        .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn attachments_defer_nested_verification() {
    let decoded = proto::note::NoteAttachments {
        attachments: vec![proto::note::NoteAttachment { scheme: u32::MAX, words: vec![] }],
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn note_script_defers_entrypoint_validation() {
    let decoded = proto::note::NoteScript {
        entrypoint: 1,
        mast: Some(miden_protocol::MastForest::new().into()),
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn note_recipient_defers_nested_script_verification() {
    let decoded = proto::note::NoteRecipient {
        serial_num: Some(Word::empty().into()),
        script: Some(proto::note::NoteScript {
            entrypoint: 1,
            mast: Some(miden_protocol::MastForest::new().into()),
        }),
        storage: Some(proto::note::NoteStorage { items: vec![] }),
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn advice_map_verification_rejects_duplicates_after_decoding() {
    let entry = proto::primitives::AdviceMapEntry {
        key: Some(Word::empty().into()),
        values: vec![],
    };
    let decoded = proto::primitives::AdviceMap { entries: vec![entry.clone(), entry] }
        .decode_fields()
        .unwrap();
    assert_eq!(decoded.entries.len(), 2);
    assert!(
        matches!(decoded.verify(), Err(miden_objects::decoded::primitives::AdviceError::DuplicateMapKey(key)) if key == Word::empty())
    );
}

#[test]
fn merkle_store_verification_rejects_duplicates_after_decoding() {
    let node = proto::primitives::MerkleStoreNode {
        value: Some(Word::empty().into()),
        left: Some(Word::empty().into()),
        right: Some(Word::empty().into()),
    };
    let decoded = proto::primitives::MerkleStore { nodes: vec![node.clone(), node] }
        .decode_fields()
        .unwrap();
    assert_eq!(decoded.nodes.len(), 2);
    assert!(
        matches!(decoded.verify(), Err(miden_objects::decoded::primitives::AdviceError::DuplicateMerkleParent(key)) if key == Word::empty())
    );
}

#[test]
fn advice_inputs_decode_nested_records_before_verification() {
    let input = miden_protocol::vm::AdviceInputs::default();
    let decoded = proto::primitives::AdviceInputs::from(&input).decode_fields().unwrap();
    assert!(decoded.advice_map.entries.is_empty());
    assert_eq!(decoded.verify().unwrap(), input);
    let error = proto::primitives::AdviceInputs {
        advice_stack: Some(proto::primitives::AdviceStack { values: vec![] }),
        advice_map: Some(proto::primitives::AdviceMap {
            entries: vec![proto::primitives::AdviceMapEntry { key: None, values: vec![] }],
        }),
        merkle_store: Some(proto::primitives::MerkleStore { nodes: vec![] }),
    }
    .decode_fields()
    .unwrap_err();
    assert!(error.to_string().starts_with("advice_map.entries[0].key: "), "{error}");
}

#[test]
fn mmr_delta_verifies_forest_size_after_decoding() {
    let decoded = proto::primitives::MmrDelta {
        forest: 3,
        update_data: vec![Word::empty().into()],
    }
    .decode_fields()
    .unwrap();
    let delta = decoded.verify().unwrap();
    assert_eq!(delta.forest.num_leaves(), 3);
    assert_eq!(delta.data, vec![Word::empty()]);
    let invalid = proto::primitives::MmrDelta { forest: u64::MAX, update_data: vec![] }
        .decode_fields()
        .unwrap();
    assert!(invalid.verify().is_err());
}

#[test]
fn account_witness_defers_path_depth_verification() {
    use miden_protocol::account::{AccountId, AccountIdVersion, AccountType, AssetCallbackFlag};
    let id = AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    );
    for (mask, valid) in [(0, false), (u64::MAX, true)] {
        let decoded = proto::account::AccountWitness {
            witness_id: Some(id.into()),
            commitment: Some(Word::empty().into()),
            path: Some(proto::primitives::SparseMerklePath {
                empty_nodes_mask: mask,
                siblings: vec![],
            }),
        }
        .decode_fields()
        .unwrap();
        assert_eq!(decoded.verify().is_ok(), valid);
    }
}

#[test]
fn vault_patch_entry_defers_asset_id_validation() {
    let decoded = proto::account::AccountVaultPatchEntry {
        asset_id: Some(Word::empty().into()),
        value: Some(Word::empty().into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.asset_id, Word::empty());
    assert!(decoded.verify().is_err());
}

#[test]
fn vault_patch_verifies_duplicate_ids_and_asset_values() {
    use miden_protocol::account::{AccountId, AccountIdVersion, AccountType, AssetCallbackFlag};
    let id = miden_protocol::asset::AssetId::new_fungible(AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    ));
    let entry = proto::account::AccountVaultPatchEntry {
        asset_id: Some(id.to_word().into()),
        value: Some(Word::from([2_u32, 0, 0, 0]).into()),
    };
    assert!(
        proto::account::AccountVaultPatch { entries: vec![entry.clone()] }
            .decode_fields()
            .unwrap()
            .verify()
            .is_ok()
    );
    let decoded = proto::account::AccountVaultPatch {
        entries: vec![entry.clone(), entry.clone()],
    }
    .decode_fields()
    .unwrap();
    assert!(
        matches!(decoded.verify(), Err(miden_objects::decoded::account::VaultPatchError::DuplicateAssetId(actual)) if actual == id)
    );
    let invalid = proto::account::AccountVaultPatchEntry {
        value: Some(Word::from([1_u32, 2, 0, 0]).into()),
        ..entry
    };
    assert!(
        proto::account::AccountVaultPatch { entries: vec![invalid] }
            .decode_fields()
            .unwrap()
            .verify()
            .is_err()
    );
}

#[test]
fn transaction_script_defers_entrypoint_validation() {
    let decoded = proto::transaction::TransactionScript {
        entrypoint: 1,
        mast: Some(miden_protocol::MastForest::new().into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.entrypoint, 1);
    assert!(decoded.verify().is_err());
}

#[test]
fn note_argument_verifies_into_tuple() {
    let id = miden_protocol::note::NoteId::from_raw(Word::empty());
    let decoded = proto::transaction::NoteArgument {
        note_id: Some((&id).into()),
        args: Some(Word::from([1_u32, 2, 3, 4]).into()),
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.verify().unwrap(), (id, Word::from([1_u32, 2, 3, 4])));
    let error = proto::transaction::NoteArgument { note_id: None, args: None }
        .decode_fields()
        .unwrap_err();
    assert!(error.to_string().starts_with("note_id: "), "{error}");
}

#[test]
fn transaction_args_decode_optional_script_without_verifying_it() {
    let input = miden_protocol::transaction::TransactionArgs::from_parts(
        None,
        Word::empty(),
        Default::default(),
        Default::default(),
        Word::empty(),
    );
    let wire = proto::transaction::TransactionArgs::from(&input);
    let decoded = wire.clone().decode_fields().unwrap();
    assert!(decoded.tx_script.is_none());
    assert_eq!(decoded.verify().unwrap(), input);
    let decoded = proto::transaction::TransactionArgs {
        tx_script: Some(proto::transaction::TransactionScript {
            entrypoint: 1,
            mast: Some(miden_protocol::MastForest::new().into()),
        }),
        ..wire
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.tx_script.is_some());
    assert!(decoded.verify().is_err());
}

#[test]
fn foreign_account_slot_name_defers_name_and_id_validation() {
    let name = miden_protocol::account::StorageSlotName::mock(1);
    let wire = proto::transaction::ForeignAccountSlotName {
        slot_id: Some(name.id().into()),
        slot_name: name.as_str().into(),
    };
    assert_eq!(wire.clone().decode_fields().unwrap().verify().unwrap(), (name.id(), name));
    let invalid =
        proto::transaction::ForeignAccountSlotName { slot_name: "".into(), ..wire.clone() };
    assert!(invalid.decode_fields().unwrap().verify().is_err());
    let mismatched = proto::transaction::ForeignAccountSlotName {
        slot_name: miden_protocol::account::StorageSlotName::mock(2).as_str().into(),
        ..wire
    };
    assert!(matches!(
        mismatched.decode_fields().unwrap().verify(),
        Err(miden_objects::decoded::transaction::ForeignAccountSlotNameError::IdMismatch { .. })
    ));
}

#[test]
fn note_inclusion_proof_defers_index_and_path_checks() {
    let id = miden_protocol::note::NoteId::from_raw(Word::empty());
    let wire = proto::note::NoteInclusionProof {
        note_id: Some((&id).into()),
        block_num: Some(miden_protocol::block::BlockNumber::from(0_u32).into()),
        note_index_in_block: u32::MAX,
        inclusion_path: Some(proto::primitives::SparseMerklePath {
            empty_nodes_mask: 0,
            siblings: vec![],
        }),
    };
    let decoded = wire.clone().decode_fields().unwrap();
    assert_eq!(decoded.note_index_in_block, u32::MAX);
    assert!(decoded.verify().is_err());
    let error = proto::note::NoteInclusionProof { inclusion_path: None, ..wire }
        .decode_fields()
        .unwrap_err();
    assert!(error.to_string().starts_with("inclusion_path: "), "{error}");
}

#[test]
fn private_account_update_verifies_empty_payload() {
    let decoded = proto::account::PrivateAccountUpdate {}.decode_fields().unwrap();
    assert_eq!(
        decoded.verify().unwrap(),
        miden_protocol::account::AccountUpdateDetails::Private
    );
}

#[test]
fn account_header_decodes_named_version_before_verifying() {
    use miden_protocol::account::{AccountId, AccountIdVersion, AccountType, AssetCallbackFlag};
    let id = AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    );
    let header = miden_protocol::account::AccountHeader::new(
        id,
        miden_protocol::Felt::ZERO,
        Word::empty(),
        Word::empty(),
        Word::empty(),
    );
    let wire = proto::account::AccountHeader::from(&header);
    let decoded = wire.clone().decode_fields().unwrap();
    assert_eq!(decoded.version, proto::account::AccountVersion::V1);
    assert_eq!(decoded.verify().unwrap(), header);
    let decoded = proto::account::AccountHeader { version: 0, ..wire.clone() }
        .decode_fields()
        .unwrap();
    assert_eq!(decoded.version, proto::account::AccountVersion::Unspecified);
    assert!(decoded.verify().is_err());
    let decoded = proto::account::AccountHeader {
        nonce: miden_protocol::Felt::ORDER,
        ..wire
    }
    .decode_fields()
    .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn storage_slot_decodes_named_type_and_defers_semantics() {
    use miden_protocol::account::{StorageSlotName, StorageSlotType};
    let wire = proto::account::account_storage_header::StorageSlot {
        slot_name: StorageSlotName::mock(1).as_str().into(),
        slot_type: proto::account::StorageSlotType::Map as i32,
        commitment: Some(Word::empty().into()),
    };
    let decoded = wire.clone().decode_fields().unwrap();
    assert_eq!(decoded.slot_type, proto::account::StorageSlotType::Map);
    assert_eq!(decoded.verify().unwrap().slot_type(), StorageSlotType::Map);
    let decoded =
        proto::account::account_storage_header::StorageSlot { slot_type: 0, ..wire.clone() }
            .decode_fields()
            .unwrap();
    assert!(decoded.verify().is_err());
    let decoded =
        proto::account::account_storage_header::StorageSlot { slot_name: "".into(), ..wire }
            .decode_fields()
            .unwrap();
    assert!(decoded.verify().is_err());
}

#[test]
fn storage_header_reports_nested_enum_paths_and_defers_duplicates() {
    let slot = proto::account::account_storage_header::StorageSlot {
        slot_name: miden_protocol::account::StorageSlotName::mock(1).as_str().into(),
        slot_type: proto::account::StorageSlotType::Value as i32,
        commitment: Some(Word::empty().into()),
    };
    let decoded = proto::account::AccountStorageHeader { slots: vec![slot.clone(), slot.clone()] }
        .decode_fields()
        .unwrap();
    assert!(decoded.verify().is_err());
    let error = proto::account::AccountStorageHeader {
        slots: vec![
            slot.clone(),
            proto::account::account_storage_header::StorageSlot { slot_type: 99, ..slot },
        ],
    }
    .decode_fields()
    .unwrap_err();
    assert!(error.to_string().starts_with("slots[1].slot_type: "), "{error}");
}

#[test]
fn storage_map_patch_verifies_operation_constraints_after_decoding() {
    use proto::account::StoragePatchOperation::{Create, Remove, Unspecified, Update};
    let entry = proto::account::StorageMapEntry {
        key: Some(Word::empty().into()),
        value: Some(Word::empty().into()),
    };
    for (operation, count, valid) in [
        (Create, 0, true),
        (Create, 1, true),
        (Update, 0, false),
        (Update, 1, true),
        (Remove, 0, true),
        (Remove, 1, false),
        (Unspecified, 0, false),
        (Create, 2, false),
    ] {
        let decoded = proto::account::StorageMapPatch {
            operation: operation as i32,
            entries: vec![entry.clone(); count],
        }
        .decode_fields()
        .unwrap();
        assert_eq!(decoded.operation, operation);
        assert_eq!(decoded.verify().is_ok(), valid, "{operation:?}, {count}");
    }
}

#[test]
fn asset_id_decodes_named_enums_before_verifying_composition() {
    use miden_protocol::asset::FungibleAsset;
    let wire = proto::asset::AssetId {
        version: proto::asset::AssetVersion::V1 as i32,
        asset_class: Some(proto::asset::AssetClass {
            suffix: Some(miden_protocol::Felt::ZERO.into()),
            prefix: Some(miden_protocol::Felt::ZERO.into()),
        }),
        composition: proto::asset::AssetComposition::Fungible as i32,
        faucet_id: Some(FungibleAsset::mock_issuer().into()),
    };
    let decoded = wire.decode_fields().unwrap();
    assert_eq!(decoded.version, proto::asset::AssetVersion::V1);
    assert_eq!(decoded.composition, proto::asset::AssetComposition::Fungible);
    assert!(decoded.verify().is_ok());
    let decoded = proto::asset::AssetId {
        composition: proto::asset::AssetComposition::Custom as i32,
        ..wire
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.composition, proto::asset::AssetComposition::Custom);
    assert!(decoded.verify().is_err());
}

#[test]
fn asset_reports_nested_enum_paths_before_verifying() {
    let wire = proto::asset::Asset {
        asset_id: Some(proto::asset::AssetId { version: 99, ..Default::default() }),
        value: Some(Word::empty().into()),
    };
    let error = wire.decode_fields().unwrap_err();
    assert!(error.to_string().starts_with("asset_id.version: "), "{error}");
}

#[test]
fn note_metadata_decodes_named_enums_and_defers_attachment_checks() {
    let metadata = *miden_protocol::note::Note::mock_noop(Word::empty()).metadata();
    let wire = proto::note::NoteMetadata::from(metadata);
    let decoded = wire.clone().decode_fields().unwrap();
    assert_eq!(decoded.version, proto::note::NoteVersion::V1);
    assert_eq!(decoded.note_type, proto::note::NoteType::Private);
    assert_eq!(decoded.verify().unwrap(), metadata);
    for attachment_schemes in
        [vec![u32::MAX], vec![0; miden_protocol::note::NoteAttachments::MAX_COUNT + 1]]
    {
        let decoded = proto::note::NoteMetadata { attachment_schemes, ..wire.clone() }
            .decode_fields()
            .unwrap();
        assert!(decoded.verify().is_err());
    }
    let decoded = proto::note::NoteMetadata { note_type: 0, ..wire }.decode_fields().unwrap();
    assert_eq!(decoded.note_type, proto::note::NoteType::Unspecified);
    assert!(decoded.verify().is_err());
}

#[test]
fn note_details_reports_repeated_asset_enum_paths() {
    let note = miden_protocol::note::Note::mock_noop(Word::empty());
    let (assets, _, recipient, _) = note.into_parts();
    let details = miden_protocol::note::NoteDetails::new(assets, recipient);
    let mut wire = proto::note::NoteDetails::from(&details);
    assert_eq!(wire.clone().decode_fields().unwrap().verify().unwrap(), details);
    let index = wire.assets.len();
    wire.assets.push(proto::asset::Asset {
        asset_id: Some(proto::asset::AssetId { version: 99, ..Default::default() }),
        value: Some(Word::empty().into()),
    });
    let error = wire.decode_fields().unwrap_err();
    assert!(
        error.to_string().starts_with(&format!("assets[{index}].asset_id.version: ")),
        "{error}"
    );
}

#[test]
fn note_header_defers_metadata_verification() {
    let header = *miden_protocol::note::Note::mock_noop(Word::empty()).header();
    let mut wire = proto::note::NoteHeader::from(header);
    assert_eq!(wire.clone().decode_fields().unwrap().verify().unwrap(), header);
    wire.metadata.as_mut().unwrap().note_type = 0;
    assert!(wire.clone().decode_fields().unwrap().verify().is_err());
    wire.metadata.as_mut().unwrap().note_type = 99;
    let error = wire.decode_fields().unwrap_err();
    assert!(error.to_string().starts_with("metadata.note_type: "), "{error}");
}

#[test]
fn input_note_commitment_builds_unchecked_after_decoding() {
    use miden_objects::BuildUnchecked;
    let header = *miden_protocol::note::Note::mock_noop(Word::empty()).header();
    for header in [None, Some(header)] {
        let wire = proto::transaction::InputNoteCommitment {
            nullifier: Some(Word::empty().into()),
            header: header.map(Into::into),
        };
        let built = wire.decode_fields().unwrap().build_unchecked().unwrap();
        assert_eq!(built.nullifier().as_word(), Word::empty());
        assert_eq!(built.header().copied(), header);
    }
    let mut wire = proto::transaction::InputNoteCommitment {
        nullifier: Some(Word::empty().into()),
        header: Some(header.into()),
    };
    wire.header.as_mut().unwrap().metadata.as_mut().unwrap().note_type = 0;
    assert!(wire.clone().decode_fields().unwrap().build_unchecked().is_err());
    wire.header.as_mut().unwrap().metadata.as_mut().unwrap().note_type = 99;
    let error = wire.decode_fields().unwrap_err();
    assert!(error.to_string().starts_with("header.metadata.note_type: "), "{error}");
}

#[test]
fn private_output_note_defers_cross_field_verification() {
    let note = miden_protocol::note::Note::mock_noop(Word::empty());
    let output = miden_protocol::transaction::PrivateOutputNote::new(
        *note.header(),
        note.attachments().clone(),
    )
    .unwrap();
    let mut wire = proto::transaction::PrivateOutputNote::from(&output);
    assert_eq!(wire.clone().decode_fields().unwrap().verify().unwrap(), output);
    wire.header.as_mut().unwrap().metadata.as_mut().unwrap().note_type =
        proto::note::NoteType::Public as i32;
    assert!(wire.decode_fields().unwrap().verify().is_err());
}

#[test]
fn transaction_header_unchecked_build_still_checks_id_and_duplicates() {
    use miden_objects::BuildUnchecked;
    use miden_protocol::account::{AccountId, AccountIdVersion, AccountType, AssetCallbackFlag};
    let id = AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    );
    let header = miden_protocol::transaction::TransactionHeader::new(
        id,
        Word::empty(),
        Word::empty(),
        Default::default(),
        vec![],
    )
    .unwrap();
    let wire = proto::transaction::TransactionHeader::from(&header);
    assert_eq!(wire.clone().decode_fields().unwrap().build_unchecked().unwrap(), header);
    let invalid = proto::transaction::TransactionHeader {
        transaction_id: Some(proto::transaction::TransactionId {
            id: Some(Word::from([1_u32, 0, 0, 0]).into()),
        }),
        ..wire.clone()
    }
    .decode_fields()
    .unwrap();
    assert!(matches!(
        invalid.build_unchecked(),
        Err(miden_objects::decoded::transaction::TransactionHeaderBuildError::IdMismatch { .. })
    ));
    let input = proto::transaction::InputNoteCommitment {
        nullifier: Some(Word::empty().into()),
        header: None,
    };
    let duplicate = proto::transaction::TransactionHeader {
        input_notes: vec![input.clone(), input],
        ..wire
    }
    .decode_fields()
    .unwrap();
    assert!(matches!(
        duplicate.build_unchecked(),
        Err(miden_objects::decoded::transaction::TransactionHeaderBuildError::Input(_))
    ));
}

#[test]
fn decoded_storage_value_patch_is_a_typed_oneof() {
    use miden_objects::proto::account::StorageValuePatch;
    use miden_objects::proto::account::storage_value_patch::Operation;
    let decoded = StorageValuePatch { operation: Some(Operation::Remove(())) }
        .decode_fields()
        .unwrap();
    assert_eq!(decoded.verify().unwrap(), miden_protocol::account::StorageValuePatch::Remove);
    assert!(StorageValuePatch::default().decode_fields().is_err());
}

#[test]
fn storage_slot_patch_defers_slot_name_validation() {
    let wire = proto::account::StorageSlotPatch {
        slot_name: String::new(),
        patch: Some(proto::account::storage_slot_patch::Patch::Value(
            proto::account::StorageValuePatch {
                operation: Some(proto::account::storage_value_patch::Operation::Remove(())),
            },
        )),
    };
    assert!(wire.decode_fields().unwrap().verify().is_err());
}

#[test]
fn smt_leaf_oneof_reports_nested_entry_errors() {
    let wire = proto::primitives::SmtLeaf {
        leaf: Some(proto::primitives::smt_leaf::Leaf::Multiple(
            proto::primitives::SmtLeafEntryList {
                entries: vec![proto::primitives::SmtLeafEntry { key: None, value: None }],
            },
        )),
    };
    let error = wire.decode_fields().unwrap_err();
    assert!(error.to_string().starts_with("leaf.multiple.entries[0].key:"), "{error}");
    let wire = proto::primitives::SmtLeaf {
        leaf: Some(proto::primitives::smt_leaf::Leaf::Multiple(
            proto::primitives::SmtLeafEntryList { entries: vec![] },
        )),
    };
    assert!(wire.decode_fields().unwrap().verify().is_err());
}

#[test]
fn public_key_oneof_rejects_missing_and_noncanonical_payloads() {
    use miden_protocol::testing::random_secret_key::random_secret_key;
    use prost::Message;
    let key = random_secret_key().public_key();
    let wire: proto::primitives::PublicKey = (&key).into();
    assert_eq!(
        proto::primitives::PublicKey::decode(wire.encode_to_vec().as_slice())
            .unwrap()
            .decode_fields()
            .unwrap()
            .verify()
            .unwrap(),
        key
    );
    assert!(proto::primitives::PublicKey::default().decode_fields().is_err());
    let mut wire: proto::primitives::PublicKey = key.into();
    let proto::primitives::public_key::Key::EcdsaK256Keccak(bytes) = wire.key.as_mut().unwrap();
    bytes.push(0);
    assert!(
        wire.decode_fields()
            .unwrap_err()
            .to_string()
            .starts_with("key.ecdsa_k256_keccak:")
    );
    // An unknown algorithm does not become the default supported algorithm.
    let wire = proto::primitives::PublicKey::decode(&[0x12, 0][..]).unwrap();
    assert!(wire.decode_fields().is_err());
}

#[test]
fn signature_oneof_rejects_missing_unknown_and_noncanonical_payloads() {
    use miden_protocol::testing::random_secret_key::random_secret_key;
    use prost::Message;
    let signature = random_secret_key().sign(miden_protocol::Word::empty());
    let wire: proto::primitives::Signature = (&signature).into();
    assert_eq!(
        proto::primitives::Signature::decode(wire.encode_to_vec().as_slice())
            .unwrap()
            .decode_fields()
            .unwrap()
            .verify()
            .unwrap(),
        signature
    );
    assert!(proto::primitives::Signature::default().decode_fields().is_err());
    let mut wire: proto::primitives::Signature = signature.into();
    let proto::primitives::signature::Signature::EcdsaK256Keccak(bytes) =
        wire.signature.as_mut().unwrap();
    bytes.push(0);
    assert!(
        wire.decode_fields()
            .unwrap_err()
            .to_string()
            .starts_with("signature.ecdsa_k256_keccak:")
    );
    assert!(
        proto::primitives::Signature::decode(&[0x12, 0][..])
            .unwrap()
            .decode_fields()
            .is_err()
    );
}

#[test]
fn partial_note_metadata_roundtrips_without_attachment_fields() {
    use miden_protocol::note::{NoteType, PartialNoteMetadata};
    let metadata = PartialNoteMetadata::new(
        miden_protocol::account::AccountId::dummy(
            [1; 15],
            miden_protocol::account::AccountIdVersion::Version1,
            miden_protocol::account::AccountType::Private,
            miden_protocol::account::AssetCallbackFlag::Disabled,
        ),
        NoteType::Public,
    );
    let wire: proto::note::PartialNoteMetadata = metadata.into();
    assert_eq!(wire.decode_fields().unwrap().verify().unwrap(), metadata);
}

#[test]
fn signed_blocks_require_a_trusted_parent_for_authentication() {
    use miden_objects::{BuildUnchecked, VerifyWith};
    use miden_protocol::block::{BlockBody, BlockHeader, SignedBlock, ValidatorConfig};
    use miden_protocol::transaction::OrderedTransactionHeaders;

    fn header_for(num: u32, previous: Word, keys: ValidatorConfig) -> BlockHeader {
        let body = BlockBody::new(
            vec![],
            vec![],
            vec![],
            OrderedTransactionHeaders::new_unchecked(vec![]),
        )
        .unwrap();
        BlockHeader::new(
            previous,
            num.into(),
            Word::empty(),
            Word::empty(),
            Word::empty(),
            body.compute_block_note_tree().root(),
            body.transaction_commitment(),
            keys,
            miden_protocol::block::FeeParameters::new(500),
            Word::empty(),
            None,
            0,
        )
    }
    let (signers, keys) = ValidatorConfig::random_with_signers(1);
    let parent = header_for(0, Word::empty(), keys.clone());
    let header = header_for(1, parent.commitment(), keys.clone());
    let body =
        BlockBody::new(vec![], vec![], vec![], OrderedTransactionHeaders::new_unchecked(vec![]))
            .unwrap();
    let block =
        SignedBlock::new(header.clone(), body, keys.sign_all(&signers, header.commitment()))
            .unwrap();
    let wire: proto::blockchain::SignedBlock = (&block).into();
    assert_eq!(wire.clone().decode_fields().unwrap().verify_with(&parent).unwrap(), block);
    let wrong_parent = header_for(0, Word::empty(), ValidatorConfig::random_with_signers(1).1);
    assert!(wire.clone().decode_fields().unwrap().verify_with(&wrong_parent).is_err());
    assert_eq!(wire.decode_fields().unwrap().build_unchecked().unwrap(), block);
}
