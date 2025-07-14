use miden_objects::crypto::dsa::rpo_falcon512;

/// Defines authentication schemes available to standard and faucet accounts.
pub enum AuthScheme {
    /// A single-key authentication scheme which relies RPO Falcon512 signatures. RPO Falcon512 is
    /// a variant of the [Falcon](https://falcon-sign.info/) signature scheme. This variant differs from
    /// the standard in that instead of using SHAKE256 hash function in the hash-to-point algorithm
    /// we use RPO256. This makes the signature more efficient to verify in Miden VM.
    RpoFalcon512 { pub_key: rpo_falcon512::PublicKey },
}
