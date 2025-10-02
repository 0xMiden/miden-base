use alloc::vec::Vec;

use miden_objects::Word;

use crate::account::auth::PublicKeyCommitment;

/// Defines authentication schemes available to standard and faucet accounts.
pub enum AuthScheme {
    /// A single-key authentication scheme which relies RPO Falcon512 signatures. RPO Falcon512 is
    /// a variant of the [Falcon](https://falcon-sign.info/) signature scheme. This variant differs from
    /// the standard in that instead of using SHAKE256 hash function in the hash-to-point algorithm
    /// we use RPO256. This makes the signature more efficient to verify in Miden VM.
    RpoFalcon512 { pub_key: PublicKeyCommitment },
    /// A multi-signature authentication scheme using RPO Falcon512 signatures.
    /// Requires a threshold number of signatures from the provided public keys.
    RpoFalcon512Multisig {
        threshold: u32,
        pub_keys: Vec<PublicKeyCommitment>,
    },
    /// A minimal authentication scheme that provides no cryptographic authentication.
    /// It only increments the nonce if the account state has actually changed during
    /// transaction execution, avoiding unnecessary nonce increments for transactions
    /// that don't modify the account state.
    NoAuth,
    /// A non-standard authentication scheme.
    Unknown,
}

impl AuthScheme {
    /// Returns all public key commitments associated with this authentication scheme as Words.
    pub fn get_public_key_commitments(&self) -> Vec<Word> {
        match self {
            AuthScheme::NoAuth => Vec::new(),
            AuthScheme::RpoFalcon512 { pub_key } => vec![Word::from(*pub_key)],
            AuthScheme::RpoFalcon512Multisig { pub_keys, .. } => {
                pub_keys.iter().map(|key| Word::from(*key)).collect()
            },
            AuthScheme::Unknown => Vec::new(),
        }
    }
}
