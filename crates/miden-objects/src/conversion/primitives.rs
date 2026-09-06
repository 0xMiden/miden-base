use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;

use miden_protocol::crypto::dsa::ecdsa_k256_keccak::{PublicKey, Signature};
use miden_protocol::crypto::merkle::store::MerkleStore;
use miden_protocol::utils::serde::{Deserializable, Serializable};
use miden_protocol::vm::{AdviceInputs, AdviceMap, AdviceStack, ExecutionProof};
use miden_protocol::{Felt, MastForest, Word};

use crate::{ConversionError, ConversionResultExt, proto};

const WORD_SERIALIZED_SIZE: usize = Word::SERIALIZED_SIZE;

fn ensure_exact_length(
    encoded: &[u8],
    expected: usize,
    field: &'static str,
) -> Result<(), ConversionError> {
    if encoded.len() != expected {
        return Err(ConversionError::message(format!(
            "expected exactly {expected} bytes, got {}",
            encoded.len()
        ))
        .context(field));
    }
    Ok(())
}

// FELT
// ================================================================================================

impl From<Felt> for proto::primitives::Felt {
    fn from(value: Felt) -> Self {
        Self { value: value.as_canonical_u64() }
    }
}

impl From<&Felt> for proto::primitives::Felt {
    fn from(value: &Felt) -> Self {
        Self { value: value.as_canonical_u64() }
    }
}

impl TryFrom<proto::primitives::Felt> for Felt {
    type Error = ConversionError;

    fn try_from(value: proto::primitives::Felt) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&proto::primitives::Felt> for Felt {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::Felt) -> Result<Self, Self::Error> {
        Self::try_from(value.value).map_err(ConversionError::new).context("felt.value")
    }
}

// WORD
// ================================================================================================

impl From<Word> for proto::primitives::Word {
    fn from(value: Word) -> Self {
        Self { encoded: value.to_bytes() }
    }
}

impl From<&Word> for proto::primitives::Word {
    fn from(value: &Word) -> Self {
        Self { encoded: value.to_bytes() }
    }
}

impl TryFrom<proto::primitives::Word> for Word {
    type Error = ConversionError;

    fn try_from(value: proto::primitives::Word) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&proto::primitives::Word> for Word {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::Word) -> Result<Self, Self::Error> {
        ensure_exact_length(&value.encoded, WORD_SERIALIZED_SIZE, "word.encoded")?;
        Self::read_from_bytes(&value.encoded)
            .map_err(|error| ConversionError::deserialization("word.encoded", error))
    }
}

// EXECUTION PROOF
// ================================================================================================

impl From<&ExecutionProof> for proto::primitives::ExecutionProof {
    fn from(value: &ExecutionProof) -> Self {
        Self { encoded: value.to_bytes() }
    }
}

impl From<ExecutionProof> for proto::primitives::ExecutionProof {
    fn from(value: ExecutionProof) -> Self {
        (&value).into()
    }
}

impl TryFrom<proto::primitives::ExecutionProof> for ExecutionProof {
    type Error = ConversionError;

    fn try_from(value: proto::primitives::ExecutionProof) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&proto::primitives::ExecutionProof> for ExecutionProof {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::ExecutionProof) -> Result<Self, Self::Error> {
        Self::read_from_bytes(&value.encoded)
            .map_err(|error| ConversionError::deserialization("ExecutionProof", error))
            .map_err(|error| error.context("encoded"))
    }
}

// MAST FOREST
// ================================================================================================

impl From<&MastForest> for proto::primitives::MastForest {
    fn from(value: &MastForest) -> Self {
        Self { encoded: value.to_bytes() }
    }
}

impl From<MastForest> for proto::primitives::MastForest {
    fn from(value: MastForest) -> Self {
        (&value).into()
    }
}

impl TryFrom<proto::primitives::MastForest> for MastForest {
    type Error = ConversionError;

    fn try_from(value: proto::primitives::MastForest) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&proto::primitives::MastForest> for MastForest {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::MastForest) -> Result<Self, Self::Error> {
        Self::read_from_bytes(&value.encoded)
            .map_err(|error| ConversionError::deserialization("MastForest", error))
            .map_err(|error| error.context("encoded"))
    }
}

// ADVICE INPUTS
// ================================================================================================

impl From<&AdviceStack> for proto::primitives::AdviceStack {
    fn from(value: &AdviceStack) -> Self {
        Self {
            values: value.iter().map(Into::into).collect(),
        }
    }
}

impl From<&AdviceMap> for proto::primitives::AdviceMap {
    fn from(value: &AdviceMap) -> Self {
        Self {
            entries: value
                .iter()
                .map(|(key, values)| proto::primitives::AdviceMapEntry {
                    key: Some(key.into()),
                    values: values.iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

impl From<&MerkleStore> for proto::primitives::MerkleStore {
    fn from(value: &MerkleStore) -> Self {
        let default_nodes = MerkleStore::new()
            .inner_nodes()
            .map(|node| (node.value, (node.left, node.right)))
            .collect::<BTreeMap<_, _>>();
        let mut nodes = value
            .inner_nodes()
            .filter(|node| default_nodes.get(&node.value) != Some(&(node.left, node.right)))
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| node.value);

        Self {
            nodes: nodes
                .into_iter()
                .map(|node| proto::primitives::MerkleStoreNode {
                    value: Some(node.value.into()),
                    left: Some(node.left.into()),
                    right: Some(node.right.into()),
                })
                .collect(),
        }
    }
}

impl From<&AdviceInputs> for proto::primitives::AdviceInputs {
    fn from(value: &AdviceInputs) -> Self {
        Self {
            advice_stack: Some((&value.stack()).into()),
            advice_map: Some(value.map().into()),
            merkle_store: Some(value.store().into()),
        }
    }
}

// PUBLIC KEY
// ================================================================================================

impl From<&PublicKey> for proto::primitives::PublicKey {
    fn from(value: &PublicKey) -> Self {
        Self {
            key: Some(proto::primitives::public_key::Key::EcdsaK256Keccak(value.to_bytes())),
        }
    }
}

impl From<PublicKey> for proto::primitives::PublicKey {
    fn from(value: PublicKey) -> Self {
        (&value).into()
    }
}

impl TryFrom<&proto::primitives::PublicKey> for PublicKey {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::PublicKey) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

// SIGNATURE
// ================================================================================================

impl From<&Signature> for proto::primitives::Signature {
    fn from(value: &Signature) -> Self {
        Self {
            signature: Some(proto::primitives::signature::Signature::EcdsaK256Keccak(
                value.to_bytes(),
            )),
        }
    }
}

impl From<Signature> for proto::primitives::Signature {
    fn from(value: Signature) -> Self {
        (&value).into()
    }
}

impl TryFrom<proto::primitives::Signature> for Signature {
    type Error = ConversionError;

    fn try_from(value: proto::primitives::Signature) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&proto::primitives::Signature> for Signature {
    type Error = ConversionError;

    fn try_from(value: &proto::primitives::Signature) -> Result<Self, Self::Error> {
        match &value.signature {
            Some(proto::primitives::signature::Signature::EcdsaK256Keccak(encoded)) => {
                Self::read_from_bytes(encoded)
                    .map_err(|error| ConversionError::deserialization("Signature", error))
                    .context("signature.ecdsa_k256_keccak")
            },
            None => {
                Err(ConversionError::missing_field::<proto::primitives::Signature>("signature"))
            },
        }
    }
}

// Canonical representation adapter; domain interpretation is left to the containing record.
impl crate::DecodeMessage for proto::primitives::Word {
    type Decoded = miden_protocol::Word;
}

// Canonical representation adapter; domain interpretation is left to the containing record.
impl crate::DecodeMessage for proto::primitives::Felt {
    type Decoded = miden_protocol::Felt;
}

// Canonical representation adapter; domain interpretation is left to the containing record.
impl crate::DecodeMessage for proto::primitives::MastForest {
    type Decoded = miden_protocol::MastForest;
}

// Canonical representation adapter; domain interpretation is left to the containing record.
impl crate::DecodeMessage for proto::primitives::ExecutionProof {
    type Decoded = miden_protocol::vm::ExecutionProof;
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;
    use core::error::Error;

    use assert_matches::assert_matches;
    use miden_protocol::testing::dummy_execution_proof;
    use miden_protocol::testing::random_secret_key::random_secret_key;
    use miden_protocol::utils::serde::DeserializationError;

    use super::*;

    #[test]
    fn felt_roundtrips_zero_and_rejects_the_field_order() {
        for felt in [Felt::ZERO, Felt::from(42_u32)] {
            let encoded = proto::primitives::Felt::from(felt);
            assert_eq!(encoded.value, felt.as_canonical_u64());
            assert_eq!(Felt::try_from(encoded).unwrap(), felt);
        }

        let error = Felt::try_from(proto::primitives::Felt { value: Felt::ORDER }).unwrap_err();
        assert_matches!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<<Felt as TryFrom<u64>>::Error>()),
            Some(source) if source.as_u64() == Felt::ORDER
        );
    }

    #[test]
    fn word_roundtrips_and_rejects_invalid_lengths() {
        let felt = Felt::from(42_u32);

        let word = Word::new([felt, Felt::ZERO, Felt::ONE, Felt::new_unchecked(7)]);
        assert_eq!(Word::try_from(proto::primitives::Word::from(word)).unwrap(), word);

        let error = Word::try_from(proto::primitives::Word { encoded: vec![0; 31] }).unwrap_err();
        assert_eq!(error.to_string(), "word.encoded: expected exactly 32 bytes, got 31");
    }

    #[test]
    fn public_key_and_signature_roundtrip_with_ecdsa_k256_keccak_variants() {
        use prost::Message;
        let signing_key = random_secret_key();
        let public_key = signing_key.public_key();
        let signature = signing_key.sign(Word::empty());

        let encoded = proto::primitives::PublicKey::from(&public_key);
        assert_matches!(encoded.key, Some(proto::primitives::public_key::Key::EcdsaK256Keccak(_)));
        let wire =
            proto::primitives::PublicKey::decode(encoded.encode_to_vec().as_slice()).unwrap();
        assert_eq!(PublicKey::try_from(wire).unwrap(), public_key);

        let encoded = proto::primitives::Signature::from(&signature);
        assert_matches!(
            encoded.signature,
            Some(proto::primitives::signature::Signature::EcdsaK256Keccak(_))
        );
        let wire =
            proto::primitives::Signature::decode(encoded.encode_to_vec().as_slice()).unwrap();
        assert_eq!(Signature::try_from(wire).unwrap(), signature);
    }

    #[test]
    fn public_key_and_signature_reject_malformed_encodings() {
        let public_key_error = PublicKey::try_from(proto::primitives::PublicKey {
            key: Some(proto::primitives::public_key::Key::EcdsaK256Keccak(vec![])),
        })
        .unwrap_err();
        assert!(public_key_error.to_string().starts_with("key.ecdsa_k256_keccak: "));
        assert_matches!(
            public_key_error
                .source()
                .and_then(|source| source.downcast_ref::<DeserializationError>()),
            Some(DeserializationError::UnexpectedEOF)
        );
        let signature_error = Signature::try_from(proto::primitives::Signature {
            signature: Some(proto::primitives::signature::Signature::EcdsaK256Keccak(vec![])),
        })
        .unwrap_err();
        assert!(signature_error.to_string().starts_with("signature.ecdsa_k256_keccak: "));
        assert_matches!(
            signature_error
                .source()
                .and_then(Error::source)
                .and_then(|source| source.downcast_ref::<DeserializationError>()),
            Some(DeserializationError::UnexpectedEOF)
        );
    }

    #[test]
    fn public_key_and_signature_require_algorithm_payloads() {
        assert!(
            PublicKey::try_from(proto::primitives::PublicKey::default())
                .unwrap_err()
                .to_string()
                .ends_with("::key is missing")
        );
        assert!(
            Signature::try_from(proto::primitives::Signature::default())
                .unwrap_err()
                .to_string()
                .ends_with("::signature is missing")
        );
    }

    #[test]
    fn public_key_and_signature_reject_unknown_algorithm_payloads() {
        use prost::Message;

        // Unknown future algorithm field 2 must not default to the supported algorithm.
        let bytes = &[0x12, 0][..];
        let key = proto::primitives::PublicKey::decode(bytes).unwrap();
        assert!(PublicKey::try_from(key).unwrap_err().to_string().ends_with("::key is missing"));
        let signature = proto::primitives::Signature::decode(bytes).unwrap();
        assert!(
            Signature::try_from(signature)
                .unwrap_err()
                .to_string()
                .ends_with("::signature is missing")
        );
    }

    #[test]
    fn execution_proof_roundtrips() {
        let proof = dummy_execution_proof();
        let encoded = proto::primitives::ExecutionProof::from(&proof);
        assert_eq!(ExecutionProof::try_from(encoded).unwrap(), proof);
    }

    #[test]
    fn execution_proof_rejects_unversioned_wire_bytes() {
        let proof = dummy_execution_proof();
        let compatibility = proof.compatibility();
        let compatibility_len = 1
            + compatibility.vm_verifier_roots().to_vec().to_bytes().len()
            + compatibility.pvm_verifier_roots().to_vec().to_bytes().len();
        let unversioned = proof.to_bytes()[compatibility_len..].to_vec();

        let error =
            ExecutionProof::try_from(proto::primitives::ExecutionProof { encoded: unversioned })
                .unwrap_err();

        assert!(error.to_string().starts_with("encoded:"));
    }

    #[test]
    fn mast_forest_roundtrips() {
        let mast = MastForest::new();
        let encoded = proto::primitives::MastForest::from(&mast);
        assert_eq!(MastForest::try_from(encoded).unwrap(), mast);
    }
}
