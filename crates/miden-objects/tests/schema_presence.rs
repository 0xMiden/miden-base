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
