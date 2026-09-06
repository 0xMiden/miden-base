use prost::Message;
use prost_types::field_descriptor_proto::Type;
use prost_types::{DescriptorProto, FileDescriptorSet};

fn message<'a>(
    descriptors: &'a FileDescriptorSet,
    package: &str,
    name: &str,
) -> &'a DescriptorProto {
    descriptors
        .file
        .iter()
        .filter(|file| file.package() == package)
        .flat_map(|file| &file.message_type)
        .find(|message| message.name() == name)
        .expect("message must be in the exported descriptor set")
}

#[test]
fn block_header_scheduled_upgrade_has_explicit_presence() {
    let descriptors = FileDescriptorSet::decode(miden_objects::FILE_DESCRIPTOR_SET).unwrap();
    let header = message(&descriptors, "blockchain", "BlockHeader");
    let upgrade = header
        .field
        .iter()
        .find(|field| field.name() == "next_protocol_config")
        .unwrap();

    assert_eq!(upgrade.number(), 13);
    assert_eq!(upgrade.r#type(), Type::Message);
    assert_eq!(upgrade.type_name(), ".blockchain.NextProtocolConfig");
    assert!(upgrade.proto3_optional());
}

#[test]
fn storage_value_patch_operations_have_distinct_payloads() {
    let descriptors = FileDescriptorSet::decode(miden_objects::FILE_DESCRIPTOR_SET).unwrap();
    let patch = message(&descriptors, "account", "StorageValuePatch");

    assert_eq!(patch.oneof_decl.len(), 1);
    assert_eq!(patch.oneof_decl[0].name(), "operation");
    assert_eq!(patch.field.len(), 3);
    for (name, number, payload) in [
        ("create", 1, ".primitives.Word"),
        ("update", 2, ".primitives.Word"),
        ("remove", 3, ".google.protobuf.Empty"),
    ] {
        let field = patch.field.iter().find(|field| field.name() == name).unwrap();
        assert_eq!(field.number(), number);
        assert_eq!(field.r#type(), Type::Message);
        assert_eq!(field.type_name(), payload);
        assert_eq!(field.oneof_index, Some(0));
        assert!(!field.proto3_optional());
    }
    assert!(message(&descriptors, "google.protobuf", "Empty").field.is_empty());
}
