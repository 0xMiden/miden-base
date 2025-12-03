use miden_core::Word;

use crate::crypto::dsa::ecdsa_k256_keccak as ecdsa;

// ECDSA SIGNER
// ================================================================================================

pub trait EcdsaSigner {
    fn sign(&self, message: Word) -> ecdsa::Signature;
    fn public_key(&self) -> ecdsa::PublicKey;
}

// IN MEMORY ECDSA SIGNER
// ================================================================================================

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
        LocalEcdsaSigner { secret_key: ecdsa::SecretKey::new() }
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
