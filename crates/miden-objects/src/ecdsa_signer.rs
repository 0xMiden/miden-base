use miden_core::Word;
use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};

use crate::crypto::dsa::ecdsa_k256_keccak as ecdsa;

// ECDSA SIGNER
// ================================================================================================

/// Trait which abstracts the signing of ECDSA signatures. Used for signing block headers.
///
/// Production-level implementations will involve some sort of secure remote backend. The trait also
/// allows for testing with local and ephemeral signers.
pub trait EcdsaSigner {
    fn sign(&self, message: Word) -> ecdsa::Signature;
    fn public_key(&self) -> ecdsa::PublicKey;
}

// IN MEMORY ECDSA SIGNER
// ================================================================================================

/// A local ECDSA signer that uses an in-memory secret key to sign messages.
///
/// Not intended for production use.
#[derive(Debug, Clone)]
pub struct LocalEcdsaSigner {
    secret_key: ecdsa::SecretKey,
}

impl LocalEcdsaSigner {
    pub fn new(secret_key: ecdsa::SecretKey) -> Self {
        LocalEcdsaSigner { secret_key }
    }

    #[cfg(any(feature = "testing", test))]
    pub fn dummy() -> Self {
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;

        let mut rng = ChaCha20Rng::from_os_rng();
        LocalEcdsaSigner {
            secret_key: ecdsa::SecretKey::with_rng(&mut rng),
        }
    }
}

impl EcdsaSigner for LocalEcdsaSigner {
    fn sign(&self, message: Word) -> ecdsa::Signature {
        self.secret_key.sign(message)
    }

    fn public_key(&self) -> ecdsa::PublicKey {
        self.secret_key.public_key()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for LocalEcdsaSigner {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.secret_key.write_into(target);
    }
}

impl Deserializable for LocalEcdsaSigner {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let secret_key = ecdsa::SecretKey::read_from(source)?;
        Ok(LocalEcdsaSigner { secret_key })
    }
}
