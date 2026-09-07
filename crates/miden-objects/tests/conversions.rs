use core::error::Error;

use assert_matches::assert_matches;
use miden_objects::{BuildUnchecked, ConversionError, DecodeMessage, Verify, proto};
use miden_protocol::account::{
    AccountHeader,
    AccountId,
    AccountIdVersion,
    AccountPatch,
    AccountStorageHeader,
    AccountStoragePatch,
    AccountType,
    AccountUpdateDetails,
    AccountVaultPatch,
    AssetCallbackFlag,
    StorageMapPatch,
    StorageSlotHeader,
    StorageSlotName,
    StorageSlotPatch,
    StorageSlotType,
    StorageValuePatch,
};
use miden_protocol::asset::{
    AssetComposition as ProtocolAssetComposition,
    FungibleAsset,
    NonFungibleAsset,
};
use miden_protocol::batch::BatchAccountUpdate;
use miden_protocol::block::account_tree::AccountWitness;
use miden_protocol::block::{
    BlockAccountUpdate,
    BlockBody,
    BlockHeader,
    BlockNumber,
    ValidatorConfig,
};
use miden_protocol::crypto::merkle::SparseMerklePath;
use miden_protocol::errors::{
    AccountIdError,
    AssetError,
    OutputNoteError,
    ProtocolConfigError,
    TransactionHeaderError,
    ValidatorConfigError,
};
use miden_protocol::note::{Note, NoteType, PartialNoteMetadata};
use miden_protocol::protocol_config::NextProtocolConfig;
use miden_protocol::transaction::{
    InputNotes,
    OrderedTransactionHeaders,
    PublicOutputNote,
    TransactionHeader,
};
use miden_protocol::{Felt, Word};
use prost::Message;

#[test]
fn protobuf_descriptor_includes_structured_asset_schema() {
    assert!(
        miden_objects::FILE_DESCRIPTOR_SET
            .windows(b"asset.proto".len())
            .any(|window| window == b"asset.proto")
    );
}

#[test]
fn fungible_asset_roundtrips_through_structured_protobuf() {
    let asset = FungibleAsset::mock(42);

    let encoded = proto::asset::Asset::from(asset);

    assert_eq!(
        encoded.asset_id.as_ref().unwrap().version,
        proto::asset::AssetVersion::V1 as i32
    );
    assert_eq!(
        encoded.asset_id.as_ref().unwrap().composition,
        proto::asset::AssetComposition::Fungible as i32
    );
    assert_eq!(encoded.decode_fields().unwrap().verify().unwrap(), asset);
}

#[test]
fn non_fungible_asset_roundtrips_through_structured_protobuf() {
    let asset = NonFungibleAsset::mock(&[1, 2, 3]);

    let encoded = proto::asset::Asset::from(asset);

    assert_eq!(
        encoded.asset_id.as_ref().unwrap().composition,
        proto::asset::AssetComposition::None as i32
    );
    assert_eq!(encoded.decode_fields().unwrap().verify().unwrap(), asset);
}

#[test]
fn structured_asset_conversion_rejects_unspecified_and_custom_compositions() {
    let asset_class = proto::asset::AssetClass {
        suffix: Some(Felt::ZERO.into()),
        prefix: Some(Felt::ZERO.into()),
    };
    let faucet_id = Some(FungibleAsset::mock_issuer().into());

    let unspecified = proto::asset::AssetId {
        version: proto::asset::AssetVersion::V1 as i32,
        asset_class: Some(asset_class),
        composition: proto::asset::AssetComposition::Unspecified as i32,
        faucet_id,
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();
    assert_eq!(unspecified.to_string(), "asset composition is unspecified");

    let custom = proto::asset::AssetId {
        version: proto::asset::AssetVersion::V1 as i32,
        asset_class: Some(asset_class),
        composition: proto::asset::AssetComposition::Custom as i32,
        faucet_id,
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();
    assert_matches!(
        custom
            .source()
            .and_then(Error::source)
            .and_then(|source| source.downcast_ref::<AssetError>()),
        Some(AssetError::UnsupportedAssetComposition(ProtocolAssetComposition::Custom))
    );
}

#[test]
fn structured_asset_conversion_rejects_nonzero_fungible_class() {
    let error = proto::asset::AssetId {
        version: proto::asset::AssetVersion::V1 as i32,
        asset_class: Some(proto::asset::AssetClass {
            suffix: Some(Felt::ONE.into()),
            prefix: Some(Felt::ZERO.into()),
        }),
        composition: proto::asset::AssetComposition::Fungible as i32,
        faucet_id: Some(FungibleAsset::mock_issuer().into()),
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_matches!(
        error
            .source()
            .and_then(Error::source)
            .and_then(|source| source.downcast_ref::<AssetError>()),
        Some(AssetError::FungibleAssetClassMustBeZero(_))
    );
}

#[test]
fn structured_asset_conversion_rejects_invalid_fungible_values() {
    let error = proto::asset::Asset {
        asset_id: Some(proto::asset::AssetId {
            version: proto::asset::AssetVersion::V1 as i32,
            asset_class: Some(proto::asset::AssetClass {
                suffix: Some(Felt::ZERO.into()),
                prefix: Some(Felt::ZERO.into()),
            }),
            composition: proto::asset::AssetComposition::Fungible as i32,
            faucet_id: Some(FungibleAsset::mock_issuer().into()),
        }),
        value: Some(Word::from([1_u32, 1, 0, 0]).into()),
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_matches!(
        error
            .source()
            .and_then(Error::source)
            .and_then(|source| source.downcast_ref::<AssetError>()),
        Some(AssetError::FungibleAssetValueMostSignificantElementsMustBeZero(_))
    );
}

#[test]
fn asset_id_protobuf_rejects_unspecified_version_after_decoding() {
    let error = proto::asset::AssetId {
        version: proto::asset::AssetVersion::Unspecified as i32,
        asset_class: Some(proto::asset::AssetClass {
            suffix: Some(Felt::ZERO.into()),
            prefix: Some(Felt::ZERO.into()),
        }),
        composition: proto::asset::AssetComposition::Fungible as i32,
        faucet_id: Some(FungibleAsset::mock_issuer().into()),
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_eq!(error.to_string(), "asset id version is unspecified");
}

fn private_account_id() -> AccountId {
    AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    )
}

fn account_witness(account_id: AccountId) -> AccountWitness {
    let path = SparseMerklePath::from_parts(u64::MAX, vec![]).unwrap();
    AccountWitness::new(account_id, Word::empty(), path).unwrap()
}

fn account_header() -> AccountHeader {
    AccountHeader::new(
        private_account_id(),
        Felt::ONE,
        Word::from([1_u32, 2, 3, 4]),
        Word::from([5_u32, 6, 7, 8]),
        Word::from([9_u32, 10, 11, 12]),
    )
}

fn account_patch() -> AccountPatch {
    AccountPatch::new(
        private_account_id(),
        AccountStoragePatch::from_entries([]).unwrap(),
        AccountVaultPatch::new([].into()).unwrap(),
        None,
        None,
    )
    .unwrap()
}

#[test]
fn account_witness_protobuf_round_trip() {
    let witness = account_witness(private_account_id());

    let message: proto::account::AccountWitness = (&witness).into();
    let decoded = message.decode_fields().unwrap().verify().unwrap();

    assert_eq!(decoded, witness);
}

#[test]
fn account_witness_conversion_preserves_account_tree_error_source() {
    let account_id = private_account_id();
    let error = proto::account::AccountWitness {
        witness_id: Some(account_id.into()),
        commitment: Some(Word::empty().into()),
        path: Some(proto::primitives::SparseMerklePath::default()),
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_matches!(
        error
            .source()
            .and_then(core::error::Error::source)
            .and_then(|source| source.downcast_ref::<miden_protocol::errors::AccountTreeError>()),
        Some(
            miden_protocol::errors::AccountTreeError::WitnessMerklePathDepthDoesNotMatchAccountTreeDepth(0)
        )
    );
}

#[test]
fn account_id_protobuf_requires_a_known_version() {
    // Field 2 represents an unknown future version. It must not default to V1.
    for bytes in [&[][..], &[0x12, 0][..]] {
        let wire = proto::account::AccountId::decode(bytes).unwrap();
        let error = wire.decode_fields().unwrap_err();
        assert_eq!(
            error.to_string(),
            "version: field miden_objects::proto::account::AccountId::version is missing"
        );
    }
}

#[test]
fn account_id_protobuf_rejects_invalid_metadata() {
    let mut wire = proto::account::AccountId::from(private_account_id());
    let proto::account::account_id::Version::V1(v1) = wire.version.as_mut().unwrap();
    v1.prefix.as_mut().unwrap().value &= !0xf;

    let error = wire
        .decode_fields()
        .unwrap()
        .verify()
        .map_err(ConversionError::new)
        .unwrap_err();

    assert_matches!(
        error.source().and_then(|source| source.downcast_ref::<AccountIdError>()),
        Some(AccountIdError::UnknownAccountIdVersion(0))
    );
}

#[test]
fn account_header_roundtrips_through_explicit_versioned_protobuf_bytes() {
    let header = account_header();

    let encoded = proto::account::AccountHeader::from(&header).encode_to_vec();
    let message = proto::account::AccountHeader::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.version, proto::account::AccountVersion::V1 as i32);
    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), header);
}

#[test]
fn account_header_protobuf_rejects_unspecified_version_after_decoding() {
    let error = proto::account::AccountHeader {
        version: proto::account::AccountVersion::Unspecified as i32,
        ..proto::account::AccountHeader::from(&account_header())
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_eq!(error.to_string(), "account header version is unspecified");
}

#[test]
fn account_header_protobuf_preserves_invalid_nonce_source() {
    let error = proto::account::AccountHeader {
        version: proto::account::AccountVersion::V1 as i32,
        account_id: Some(private_account_id().into()),
        vault_root: Some(Word::empty().into()),
        storage_commitment: Some(Word::empty().into()),
        code_commitment: Some(Word::empty().into()),
        nonce: Felt::ORDER,
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert!(error.to_string().starts_with("invalid account nonce: "));
    assert_matches!(
        error
            .source()
            .and_then(Error::source)
            .and_then(|source| source.downcast_ref::<<Felt as TryFrom<u64>>::Error>()),
        Some(source) if source.as_u64() == Felt::ORDER
    );
}

#[test]
fn account_patch_roundtrips_through_explicit_versioned_protobuf_bytes() {
    let patch = account_patch();

    let encoded = proto::account::AccountPatch::from(&patch).encode_to_vec();
    let message = proto::account::AccountPatch::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.version, proto::account::AccountPatchVersion::V1 as i32);
    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), patch);
}

#[test]
fn account_patch_protobuf_rejects_unspecified_version_after_decoding() {
    let error = proto::account::AccountPatch {
        version: proto::account::AccountPatchVersion::Unspecified as i32,
        ..proto::account::AccountPatch::from(account_patch())
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_eq!(error.to_string(), "account patch version is unspecified");
}

#[test]
fn note_metadata_roundtrips_through_flat_v1_protobuf_bytes() {
    let metadata = *Note::mock_noop(Word::empty()).metadata();

    let encoded = proto::note::NoteMetadata::from(metadata).encode_to_vec();
    let message = proto::note::NoteMetadata::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.version, proto::note::NoteVersion::V1 as i32);
    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), metadata);
}

#[test]
fn note_protobuf_roundtrips_through_versioned_note_metadata() {
    let note = Note::mock_noop(Word::empty());

    let encoded = proto::note::Note::from(note.clone()).encode_to_vec();
    let message = proto::note::Note::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), note);
}

#[test]
fn note_metadata_protobuf_rejects_unspecified_version_after_decoding() {
    let error = proto::note::NoteMetadata {
        version: proto::note::NoteVersion::Unspecified as i32,
        ..proto::note::NoteMetadata::from(*Note::mock_noop(Word::empty()).metadata())
    }
    .decode_fields()
    .unwrap()
    .verify()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_eq!(error.to_string(), "note metadata version is unspecified");
}

#[test]
fn note_protobuf_rejects_unspecified_metadata_version_after_decoding() {
    let mut message = proto::note::Note::from(Note::mock_noop(Word::empty()));
    message.metadata.as_mut().unwrap().version = proto::note::NoteVersion::Unspecified as i32;
    let error = message
        .decode_fields()
        .unwrap()
        .verify()
        .map_err(ConversionError::new)
        .unwrap_err();
    assert_eq!(error.to_string(), "note metadata version is unspecified");
}
#[test]
fn note_protobuf_reconstructs_attachment_metadata_from_structured_attachments() {
    let note = Note::mock_noop(Word::empty());
    let message = proto::note::Note::from(note.clone());
    assert_eq!(message.metadata.as_ref().unwrap().tag, note.metadata().tag().as_u32());
    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), note);
}

fn public_note() -> Note {
    let (assets, metadata, recipient, attachments) = Note::mock_noop(Word::empty()).into_parts();
    let metadata =
        PartialNoteMetadata::new(metadata.sender(), NoteType::Public).with_tag(metadata.tag());

    Note::with_attachments(assets, metadata, recipient, attachments)
}

#[test]
fn public_output_note_roundtrips_through_protobuf() {
    let note = PublicOutputNote::new(public_note()).unwrap();

    let encoded = proto::transaction::PublicOutputNote::from(note.clone()).encode_to_vec();
    let message = proto::transaction::PublicOutputNote::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), note);
}

#[test]
fn public_output_note_protobuf_rejects_private_note() {
    let note = Note::mock_noop(Word::empty());
    let error = proto::transaction::PublicOutputNote { note: Some(note.clone().into()) }
        .decode_fields()
        .unwrap()
        .verify()
        .map_err(ConversionError::new)
        .unwrap_err();

    assert_matches!(
        error_source::<OutputNoteError>(&error),
        Some(OutputNoteError::NoteIsPrivate(note_id)) if *note_id == note.id()
    );
}

#[test]
fn account_update_roundtrips_through_protobuf_bytes() {
    let update = BatchAccountUpdate::new(
        private_account_id(),
        Word::from([1_u32, 2, 3, 4]),
        Word::from([5_u32, 6, 7, 8]),
        AccountUpdateDetails::Private,
    )
    .unwrap();

    let encoded = proto::transaction::BatchAccountUpdate::from(&update).encode_to_vec();
    let message = proto::transaction::BatchAccountUpdate::decode(encoded.as_slice()).unwrap();
    assert_eq!(message.decode_fields().unwrap().verify().unwrap(), update);
}

#[test]
fn block_body_and_transaction_header_roundtrip() {
    let account_id = private_account_id();
    let transaction = TransactionHeader::new(
        account_id,
        Word::from([1_u32, 2, 3, 4]),
        Word::from([5_u32, 6, 7, 8]),
        InputNotes::default(),
        vec![],
    )
    .unwrap();
    let account_update = BlockAccountUpdate::new(
        account_id,
        transaction.final_state_commitment(),
        AccountUpdateDetails::Private,
    )
    .unwrap();
    let body = BlockBody::new(
        vec![account_update],
        vec![],
        vec![],
        OrderedTransactionHeaders::new_unchecked(vec![transaction]),
    )
    .unwrap();

    let encoded = proto::blockchain::BlockBody::from(&body).encode_to_vec();
    let message = proto::blockchain::BlockBody::decode(encoded.as_slice()).unwrap();
    assert_eq!(message.decode_fields().unwrap().build_unchecked().unwrap(), body);
}

#[test]
fn account_storage_header_rejects_unspecified_slot_types() {
    let message = proto::account::AccountStorageHeader {
        slots: vec![proto::account::account_storage_header::StorageSlot {
            slot_name: "miden::test::storage".into(),
            slot_type: proto::account::StorageSlotType::Unspecified as i32,
            commitment: Some(Word::empty().into()),
        }],
    };
    let error = message.decode_fields().unwrap().verify().unwrap_err();
    assert_eq!(error.to_string(), "storage slot type is unspecified");
}

#[test]
fn account_storage_header_uses_generated_slot_type_values() {
    for (slot_type, expected_slot_type) in [
        (StorageSlotType::Value, proto::account::StorageSlotType::Value),
        (StorageSlotType::Map, proto::account::StorageSlotType::Map),
    ] {
        let header = AccountStorageHeader::new(vec![StorageSlotHeader::new(
            StorageSlotName::new("miden::test::storage").unwrap(),
            slot_type,
            Word::empty(),
        )])
        .unwrap();

        let message = proto::account::AccountStorageHeader::from(&header);
        assert_eq!(message.slots[0].slot_type, expected_slot_type as i32);
        assert_eq!(message.decode_fields().unwrap().verify().unwrap(), header);
    }
}

#[test]
fn account_storage_patch_protobuf_slots_follow_canonical_storage_order() {
    let storage_patch = AccountStoragePatch::from_entries([
        (StorageSlotName::mock(3), StorageSlotPatch::Value(StorageValuePatch::Remove)),
        (StorageSlotName::mock(1), StorageSlotPatch::Map(StorageMapPatch::Remove)),
        (StorageSlotName::mock(4), StorageSlotPatch::Value(StorageValuePatch::Remove)),
        (StorageSlotName::mock(2), StorageSlotPatch::Map(StorageMapPatch::Remove)),
    ])
    .unwrap();

    let expected_slots = [
        ("miden::test::slot::3", true),
        ("miden::test::slot::1", false),
        ("miden::test::slot::4", true),
        ("miden::test::slot::2", false),
    ];
    let message = proto::account::AccountStoragePatch::from(&storage_patch);

    assert_eq!(
        message
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.slot_name.as_str(),
                    matches!(
                        slot.patch.as_ref(),
                        Some(proto::account::storage_slot_patch::Patch::Value(_))
                    ),
                )
            })
            .collect::<Vec<_>>(),
        expected_slots
    );
}

#[test]
fn storage_value_patch_oneof_roundtrips_all_operations() {
    use proto::account::storage_value_patch::Operation;

    for value in [Word::empty(), Word::from([1_u32, 2, 3, 4])] {
        for (patch, expected) in [
            (StorageValuePatch::Create { value }, Operation::Create(value.into())),
            (StorageValuePatch::Update { value }, Operation::Update(value.into())),
            (StorageValuePatch::Remove, Operation::Remove(())),
        ] {
            let message = proto::account::StorageValuePatch::from(&patch);
            assert_eq!(message.operation, Some(expected));
            let encoded = message.encode_to_vec();
            let decoded = proto::account::StorageValuePatch::decode(encoded.as_slice()).unwrap();
            assert_eq!(decoded.decode_fields().unwrap().verify().unwrap(), patch);
        }
    }

    let remove = proto::account::StorageValuePatch::from(&StorageValuePatch::Remove);
    assert_eq!(remove.encode_to_vec(), [0x1a, 0x00]);
}

#[test]
fn storage_value_patch_nested_in_account_storage_patch_roundtrips() {
    let patch = AccountStoragePatch::from_entries([
        (
            StorageSlotName::mock(1),
            StorageSlotPatch::Value(StorageValuePatch::Create { value: Word::empty() }),
        ),
        (
            StorageSlotName::mock(2),
            StorageSlotPatch::Value(StorageValuePatch::Update {
                value: Word::from([1_u32, 0, 0, 0]),
            }),
        ),
        (StorageSlotName::mock(3), StorageSlotPatch::Value(StorageValuePatch::Remove)),
    ])
    .unwrap();
    let message = proto::account::AccountStoragePatch::from(&patch);
    let encoded = message.encode_to_vec();
    let decoded = proto::account::AccountStoragePatch::decode(encoded.as_slice()).unwrap();

    assert_eq!(decoded.decode_fields().unwrap().verify().unwrap(), patch);
}

#[test]
fn empty_protobuf_block_body_decodes_to_an_empty_domain_body() {
    let expected =
        BlockBody::new(vec![], vec![], vec![], OrderedTransactionHeaders::new_unchecked(vec![]))
            .unwrap();

    assert_eq!(
        proto::blockchain::BlockBody::default()
            .decode_fields()
            .unwrap()
            .build_unchecked()
            .unwrap(),
        expected
    );
}

#[test]
fn block_header_protobuf_rejects_unspecified_version_after_decoding() {
    let error = proto::blockchain::BlockHeader {
        version: proto::blockchain::BlockVersion::Unspecified as i32,
        ..proto::blockchain::BlockHeader::from(block_header_with_scheduled_upgrade())
    }
    .decode_fields()
    .unwrap()
    .build_unchecked()
    .map_err(ConversionError::new)
    .unwrap_err();

    assert_eq!(error.to_string(), "block header version is unspecified");
}

fn block_header_with_scheduled_upgrade() -> BlockHeader {
    let header = BlockHeader::mock(1, None, None, &[]);
    let (_, validator_config) = ValidatorConfig::random_with_signers(3);
    let next_protocol_config =
        NextProtocolConfig::new(BlockNumber::from(42u32), Word::from([9u32, 8, 7, 6])).unwrap();

    BlockHeader::new(
        header.prev_block_commitment(),
        header.block_num(),
        header.chain_commitment(),
        header.account_root(),
        header.nullifier_root(),
        header.note_root(),
        header.tx_commitment(),
        validator_config,
        header.fee_parameters().clone(),
        header.protocol_config_commitment(),
        Some(next_protocol_config),
        header.timestamp(),
    )
}

#[test]
fn block_header_protobuf_round_trip_preserves_current_fields() {
    let header = block_header_with_scheduled_upgrade();

    let encoded = proto::blockchain::BlockHeader::from(&header).encode_to_vec();
    let message = proto::blockchain::BlockHeader::decode(encoded.as_slice()).unwrap();

    assert_eq!(message.version, proto::blockchain::BlockVersion::V1 as i32);
    assert_eq!(message.decode_fields().unwrap().build_unchecked().unwrap(), header);
}

#[test]
fn block_header_protobuf_round_trip_preserves_absent_scheduled_upgrade() {
    let header = BlockHeader::mock(1, None, None, &[]);
    assert!(header.next_protocol_config().is_none());

    let encoded = proto::blockchain::BlockHeader::from(&header).encode_to_vec();
    let message = proto::blockchain::BlockHeader::decode(encoded.as_slice()).unwrap();

    assert!(message.next_protocol_config.is_none());
    assert_eq!(message.decode_fields().unwrap().build_unchecked().unwrap(), header);
}

#[test]
fn block_header_protobuf_rejects_invalid_validator_quorum() {
    let header = block_header_with_scheduled_upgrade();
    let mut message = proto::blockchain::BlockHeader::from(header);
    message.validator_config.as_mut().unwrap().quorum = 0;

    let error = message
        .decode_fields()
        .unwrap()
        .build_unchecked()
        .map_err(ConversionError::new)
        .unwrap_err();
    let source = error_source::<ValidatorConfigError>(&error).unwrap();

    assert_matches!(
        source,
        ValidatorConfigError::QuorumMustEqualValidatorCount { quorum: 0, count: 3 }
    );
}

#[test]
fn block_header_protobuf_reports_invalid_validator_key_index() {
    let header = block_header_with_scheduled_upgrade();
    let mut message = proto::blockchain::BlockHeader::from(header);
    message.validator_config.as_mut().unwrap().keys[1].key =
        Some(proto::primitives::public_key::Key::EcdsaK256Keccak(vec![]));

    let error = message.decode_fields().unwrap_err();

    assert!(
        error
            .to_string()
            .starts_with("validator_config.keys[1].key.ecdsa_k256_keccak: ")
    );
    assert!(
        error
            .source()
            .unwrap()
            .is::<miden_protocol::utils::serde::DeserializationError>()
    );
}

#[test]
fn block_header_protobuf_rejects_upgrade_effective_at_genesis() {
    let header = block_header_with_scheduled_upgrade();
    let mut message = proto::blockchain::BlockHeader::from(header);
    message.next_protocol_config.as_mut().unwrap().effective_from =
        Some(BlockNumber::GENESIS.into());

    let error = message
        .decode_fields()
        .unwrap()
        .build_unchecked()
        .map_err(ConversionError::new)
        .unwrap_err();
    let source = error_source::<ProtocolConfigError>(&error).unwrap();

    assert_matches!(source, ProtocolConfigError::NextConfigEffectiveAtGenesis);
}

#[test]
fn transaction_header_conversion_preserves_validation_error_source() {
    let note = Note::mock_noop(Word::empty());
    let transaction = TransactionHeader::new(
        private_account_id(),
        Word::from([1_u32, 2, 3, 4]),
        Word::from([5_u32, 6, 7, 8]),
        InputNotes::default(),
        vec![*note.header()],
    )
    .unwrap();
    let mut message = proto::transaction::TransactionHeader::from(transaction);
    message.output_notes.push(message.output_notes[0].clone());

    let error = message
        .decode_fields()
        .unwrap()
        .build_unchecked()
        .map_err(ConversionError::new)
        .unwrap_err();
    let source = error
        .source()
        .and_then(Error::source)
        .unwrap()
        .downcast_ref::<TransactionHeaderError>()
        .unwrap();

    assert_matches!(
        source,
        TransactionHeaderError::DuplicateOutputNote(note_id) if *note_id == note.id()
    );
}

fn error_source<'a, T: Error + 'static>(error: &'a (dyn Error + 'static)) -> Option<&'a T> {
    let mut source = Some(error);
    while let Some(error) = source {
        if let Some(found) = error.downcast_ref::<T>() {
            return Some(found);
        }
        source = error.source();
    }
    None
}
