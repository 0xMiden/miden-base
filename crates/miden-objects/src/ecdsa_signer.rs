use miden_core::Word;
use miden_crypto::dsa::ecdsa_k256_keccak::SecretKey;

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

// SECRET KEY SIGNER
// ================================================================================================

impl EcdsaSigner for SecretKey {
    fn sign(&self, message: Word) -> ecdsa::Signature {
        self.sign(message)
    }

    fn public_key(&self) -> ecdsa::PublicKey {
        self.public_key()
    }
}
