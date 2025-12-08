// NO STD ECDSA SIGNER
// ================================================================================================

use miden_crypto::dsa::ecdsa_k256_keccak::SecretKey;

use crate::ecdsa_signer::EcdsaSigner;

pub trait RandomEcdsaSigner: EcdsaSigner {
    fn random() -> Self;
}

// NO STD SECRET KEY SIGNER
// ================================================================================================

impl RandomEcdsaSigner for SecretKey {
    fn random() -> Self {
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;
        let mut rng = ChaCha20Rng::from_os_rng();
        SecretKey::with_rng(&mut rng)
    }
}
