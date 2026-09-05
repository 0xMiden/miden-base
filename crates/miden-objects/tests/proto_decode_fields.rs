use core::convert::Infallible;

use miden_objects::{BuildUnchecked, DecodeMessage, VerifyWith};
use miden_protobuf::ProtoDecodeFields;

#[derive(Clone, PartialEq, prost::Message, ProtoDecodeFields)]
struct Leaf {
    #[prost(uint32, tag = "1")]
    value: u32,
}
#[derive(Clone, PartialEq, prost::Message, ProtoDecodeFields)]
struct Child {
    #[prost(message, optional, tag = "1")]
    leaf: Option<Leaf>,
}
#[derive(Clone, PartialEq, prost::Message, ProtoDecodeFields)]
struct Container {
    #[prost(message, optional, tag = "1")]
    required: Option<Child>,
    #[prost(message, optional, tag = "2")]
    #[proto_decode(optional)]
    optional: Option<Child>,
    #[prost(message, repeated, tag = "3")]
    repeated: Vec<Child>,
}
fn child() -> Child {
    Child { leaf: Some(Leaf { value: 7 }) }
}
#[test]
fn generated_records_preserve_presence_and_nested_fields() {
    let decoded = Container {
        required: Some(child()),
        optional: None,
        repeated: vec![child()],
    }
    .decode_fields()
    .unwrap();
    assert_eq!(decoded.required.leaf.value, 7);
    assert!(decoded.optional.is_none());
    assert_eq!(decoded.repeated[0].leaf.value, 7);
}
#[test]
fn generated_records_report_complete_paths() {
    let error = Container {
        required: Some(child()),
        optional: None,
        repeated: vec![child(), Child { leaf: None }],
    }
    .decode_fields()
    .unwrap_err();
    assert!(error.to_string().starts_with("repeated[1].leaf:"), "{error}");
    let error = Container::default().decode_fields().unwrap_err();
    assert!(error.to_string().starts_with("required:"), "{error}");
}
#[derive(Debug, thiserror::Error)]
#[error("value exceeds permitted limit")]
struct LimitExceeded;

impl VerifyWith<&u32> for DecodedLeaf {
    type Verified = u32;
    type Error = LimitExceeded;
    fn verify_with(self, max: &u32) -> Result<u32, Self::Error> {
        if self.value > *max {
            return Err(LimitExceeded);
        }
        Ok(self.value)
    }
}
impl BuildUnchecked for DecodedLeaf {
    type Output = u32;
    type Error = Infallible;
    // This fixture retains the decoded scalar without applying the contextual cap.
    fn build_unchecked(self) -> Result<u32, Infallible> {
        Ok(self.value)
    }
}
#[test]
fn construction_capabilities_are_independent_and_opt_in() {
    assert_eq!(Leaf { value: 7 }.decode_fields().unwrap().verify_with(&10).unwrap(), 7);
    assert!(Leaf { value: u32::MAX }.decode_fields().unwrap().verify_with(&5).is_err());
    assert_eq!(
        Leaf { value: u32::MAX }.decode_fields().unwrap().build_unchecked().unwrap(),
        u32::MAX
    );
}
