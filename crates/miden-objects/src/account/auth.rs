use alloc::vec::Vec;

use rand::{CryptoRng, Rng};

use crate::crypto::dsa::{ecdsa_k256_keccak, rpo_falcon512};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{AuthSchemeError, Felt, Hasher, Word};

// AUTH SCHEME
// ================================================================================================

/// Identifier of signature schemes use for transaction authentication
const RPO_FALCON_512: u8 = 0;
const ECDSA_K256_KECCAK: u8 = 1;

/// Defines standard authentication schemes (i.e., signature schemes) available in the Miden
/// protocol.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[repr(u8)]
pub enum AuthScheme {
    /// A deterministic RPO Falcon512 signature scheme.
    ///
    /// This version differs from the reference Falcon512 implementation in its use of the RPO
    /// algebraic hash function in its hash-to-point algorithm to make signatures very efficient
    /// to verify inside Miden VM.
    RpoFalcon512 = RPO_FALCON_512,

    /// ECDSA signature scheme over secp256k1 curve using Keccak to hash the messages when signing.
    EcdsaK256Keccak = ECDSA_K256_KECCAK,
}

impl AuthScheme {
    /// Returns a numerical value of this auth scheme.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

impl core::fmt::Display for AuthScheme {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RpoFalcon512 => f.write_str("RpoFalcon512"),
            Self::EcdsaK256Keccak => f.write_str("EcdsaK256Keccak"),
        }
    }
}

impl TryFrom<u8> for AuthScheme {
    type Error = AuthSchemeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            RPO_FALCON_512 => Ok(Self::RpoFalcon512),
            ECDSA_K256_KECCAK => Ok(Self::EcdsaK256Keccak),
            value => Err(AuthSchemeError::InvalidAuthSchemeIdentifier(value)),
        }
    }
}

impl Serializable for AuthScheme {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(*self as u8);
    }

    fn get_size_hint(&self) -> usize {
        // auth scheme is encoded as a single byte
        size_of::<u8>()
    }
}

impl Deserializable for AuthScheme {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read_u8()? {
            RPO_FALCON_512 => Ok(Self::RpoFalcon512),
            ECDSA_K256_KECCAK => Ok(Self::EcdsaK256Keccak),
            value => Err(DeserializationError::InvalidValue(format!(
                "auth scheme identifier `{value}` is not valid"
            ))),
        }
    }
}

// AUTH SECRET KEY
// ================================================================================================

/// Secret keys of the standard [`AuthScheme`]s available in the Miden protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[repr(u8)]
pub enum AuthSecretKey {
    RpoFalcon512(rpo_falcon512::SecretKey) = RPO_FALCON_512,
    EcdsaK256Keccak(ecdsa_k256_keccak::SecretKey) = ECDSA_K256_KECCAK,
}

impl AuthSecretKey {
    /// Generates an RpoFalcon512 secret key from the OS-provided randomness.
    #[cfg(feature = "std")]
    pub fn new_rpo_falcon512() -> Self {
        Self::RpoFalcon512(rpo_falcon512::SecretKey::new())
    }

    /// Generates an RpoFalcon512 secrete key using the provided random number generator.
    pub fn new_rpo_falcon512_with_rng<R: Rng>(rng: &mut R) -> Self {
        Self::RpoFalcon512(rpo_falcon512::SecretKey::with_rng(rng))
    }

    /// Generates an EcdsaK256Keccak secret key from the OS-provided randomness.
    #[cfg(feature = "std")]
    pub fn new_ecdsa_ke256_keccak() -> Self {
        Self::EcdsaK256Keccak(ecdsa_k256_keccak::SecretKey::new())
    }

    /// Generates an RpoFalcon512 secrete key using the provided random number generator.
    pub fn new_ecdsa_ke256_keccak_with_rng<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        Self::EcdsaK256Keccak(ecdsa_k256_keccak::SecretKey::with_rng(rng))
    }

    /// Returns the authentication scheme of this secret key.
    pub fn auth_scheme(&self) -> AuthScheme {
        match self {
            AuthSecretKey::RpoFalcon512(_) => AuthScheme::RpoFalcon512,
            AuthSecretKey::EcdsaK256Keccak(_) => AuthScheme::EcdsaK256Keccak,
        }
    }

    /// Returns a public key associated with this secret key.
    pub fn public_key(&self) -> PublicKey {
        match self {
            AuthSecretKey::RpoFalcon512(key) => PublicKey::RpoFalcon512(key.public_key()),
            AuthSecretKey::EcdsaK256Keccak(key) => PublicKey::EcdsaK256Keccak(key.public_key()),
        }
    }

    /// Signs the provided message with this secret key.
    pub fn sign(&self, message: Word) -> Signature {
        match self {
            AuthSecretKey::RpoFalcon512(key) => Signature::RpoFalcon512(key.sign(message)),
            AuthSecretKey::EcdsaK256Keccak(key) => {
                // TODO: cloning is not necessary, but this needs to be fixed in miden-crypto first
                Signature::EcdsaK256Keccak(key.clone().sign(message))
            },
        }
    }
}

impl Serializable for AuthSecretKey {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.auth_scheme().write_into(target);
        match self {
            AuthSecretKey::RpoFalcon512(key) => key.write_into(target),
            AuthSecretKey::EcdsaK256Keccak(key) => key.write_into(target),
        }
    }
}

impl Deserializable for AuthSecretKey {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read::<AuthScheme>()? {
            AuthScheme::RpoFalcon512 => {
                let secret_key = rpo_falcon512::SecretKey::read_from(source)?;
                Ok(AuthSecretKey::RpoFalcon512(secret_key))
            },
            AuthScheme::EcdsaK256Keccak => {
                let secret_key = ecdsa_k256_keccak::SecretKey::read_from(source)?;
                Ok(AuthSecretKey::EcdsaK256Keccak(secret_key))
            },
        }
    }
}

// PUBLIC KEY
// ================================================================================================

/// Commitment to a public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublicKeyCommitment(Word);

impl core::fmt::Display for PublicKeyCommitment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<rpo_falcon512::PublicKey> for PublicKeyCommitment {
    fn from(value: rpo_falcon512::PublicKey) -> Self {
        Self(value.to_commitment())
    }
}

impl From<PublicKeyCommitment> for Word {
    fn from(value: PublicKeyCommitment) -> Self {
        value.0
    }
}

impl From<Word> for PublicKeyCommitment {
    fn from(value: Word) -> Self {
        Self(value)
    }
}

/// Public keys of the standard authentication schemes available in the Miden protocol.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum PublicKey {
    RpoFalcon512(rpo_falcon512::PublicKey),
    EcdsaK256Keccak(ecdsa_k256_keccak::PublicKey),
}

impl PublicKey {
    /// Returns the authentication scheme of this public key.
    pub fn auth_scheme(&self) -> AuthScheme {
        match self {
            PublicKey::RpoFalcon512(_) => AuthScheme::RpoFalcon512,
            PublicKey::EcdsaK256Keccak(_) => AuthScheme::EcdsaK256Keccak,
        }
    }

    /// Returns a commitment to this public key.
    pub fn to_commitment(&self) -> PublicKeyCommitment {
        match self {
            PublicKey::RpoFalcon512(key) => key.to_commitment().into(),
            PublicKey::EcdsaK256Keccak(key) => key.to_commitment().into(),
        }
    }

    /// Verifies the provided signature against the provided message and this public key.
    pub fn verify(&self, message: Word, signature: Signature) -> bool {
        match (self, signature) {
            (PublicKey::RpoFalcon512(key), Signature::RpoFalcon512(sig)) => {
                key.verify(message, &sig)
            },
            (PublicKey::EcdsaK256Keccak(key), Signature::EcdsaK256Keccak(sig)) => {
                key.verify(message, &sig)
            },
            _ => false,
        }
    }
}

impl Serializable for PublicKey {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.auth_scheme().write_into(target);
        match self {
            PublicKey::RpoFalcon512(pub_key) => pub_key.write_into(target),
            PublicKey::EcdsaK256Keccak(pub_key) => pub_key.write_into(target),
        }
    }
}

impl Deserializable for PublicKey {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read::<AuthScheme>()? {
            AuthScheme::RpoFalcon512 => {
                let pub_key = rpo_falcon512::PublicKey::read_from(source)?;
                Ok(PublicKey::RpoFalcon512(pub_key))
            },
            AuthScheme::EcdsaK256Keccak => {
                let pub_key = ecdsa_k256_keccak::PublicKey::read_from(source)?;
                Ok(PublicKey::EcdsaK256Keccak(pub_key))
            },
        }
    }
}

// SIGNATURE
// ================================================================================================

/// Represents a signature object ready for native verification.
///
/// In order to use this signature within the Miden VM, a preparation step may be necessary to
/// convert the native signature into a vector of field elements that can be loaded into the advice
/// provider. To prepare the signature, use the provided `to_prepared_signature` method:
/// ```rust,no_run
/// use miden_objects::account::auth::Signature;
/// use miden_objects::crypto::dsa::rpo_falcon512::SecretKey;
/// use miden_objects::{Felt, Word};
///
/// let secret_key = SecretKey::new();
/// let message = Word::default();
/// let signature: Signature = secret_key.sign(message).into();
/// let prepared_signature: Vec<Felt> = signature.to_prepared_signature();
/// ```
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum Signature {
    RpoFalcon512(rpo_falcon512::Signature) = RPO_FALCON_512,
    EcdsaK256Keccak(ecdsa_k256_keccak::Signature) = ECDSA_K256_KECCAK,
}

impl Signature {
    /// Returns the authentication scheme of this signature.
    pub fn auth_scheme(&self) -> AuthScheme {
        match self {
            Signature::RpoFalcon512(_) => AuthScheme::RpoFalcon512,
            Signature::EcdsaK256Keccak(_) => AuthScheme::EcdsaK256Keccak,
        }
    }

    /// Converts this signature to a sequence of field elements in the format expected by the
    /// native verification procedure in the VM.
    pub fn to_prepared_signature(&self) -> Vec<Felt> {
        match self {
            Signature::RpoFalcon512(sig) => prepare_rpo_falcon512_signature(sig),
            Signature::EcdsaK256Keccak(sig) => prepare_ecdsa_k256_keccak_signature(sig),
        }
    }
}

impl From<rpo_falcon512::Signature> for Signature {
    fn from(signature: rpo_falcon512::Signature) -> Self {
        Signature::RpoFalcon512(signature)
    }
}

impl Serializable for Signature {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.auth_scheme().write_into(target);
        match self {
            Signature::RpoFalcon512(signature) => signature.write_into(target),
            Signature::EcdsaK256Keccak(signature) => signature.write_into(target),
        }
    }
}

impl Deserializable for Signature {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read::<AuthScheme>()? {
            AuthScheme::RpoFalcon512 => {
                let signature = rpo_falcon512::Signature::read_from(source)?;
                Ok(Signature::RpoFalcon512(signature))
            },
            AuthScheme::EcdsaK256Keccak => {
                let signature = ecdsa_k256_keccak::Signature::read_from(source)?;
                Ok(Signature::EcdsaK256Keccak(signature))
            },
        }
    }
}

// SIGNATURE PREPARATION
// ================================================================================================

/// Converts a Falcon [rpo_falcon512::Signature] to a vector of values to be pushed onto the
/// advice stack. The values are the ones required for a Falcon signature verification inside the VM
/// and they are:
///
/// 1. The challenge point at which we evaluate the polynomials in the subsequent three bullet
///    points, i.e. `h`, `s2` and `pi`, to check the product relationship.
/// 2. The expanded public key represented as the coefficients of a polynomial `h` of degree < 512.
/// 3. The signature represented as the coefficients of a polynomial `s2` of degree < 512.
/// 4. The product of the above two polynomials `pi` in the ring of polynomials with coefficients in
///    the Miden field.
/// 5. The nonce represented as 8 field elements.
fn prepare_rpo_falcon512_signature(sig: &rpo_falcon512::Signature) -> Vec<Felt> {
    use rpo_falcon512::Polynomial;

    // The signature is composed of a nonce and a polynomial s2
    // The nonce is represented as 8 field elements.
    let nonce = sig.nonce();
    // We convert the signature to a polynomial
    let s2 = sig.sig_poly();
    // We also need in the VM the expanded key corresponding to the public key that was provided
    // via the operand stack
    let h = sig.public_key();
    // Lastly, for the probabilistic product routine that is part of the verification procedure,
    // we need to compute the product of the expanded key and the signature polynomial in
    // the ring of polynomials with coefficients in the Miden field.
    let pi = Polynomial::mul_modulo_p(h, s2);

    // We now push the expanded key, the signature polynomial, and the product of the
    // expanded key and the signature polynomial to the advice stack. We also push
    // the challenge point at which the previous polynomials will be evaluated.
    // Finally, we push the nonce needed for the hash-to-point algorithm.

    let mut polynomials: Vec<Felt> =
        h.coefficients.iter().map(|a| Felt::from(a.value() as u32)).collect();
    polynomials.extend(s2.coefficients.iter().map(|a| Felt::from(a.value() as u32)));
    polynomials.extend(pi.iter().map(|a| Felt::new(*a)));

    let digest_polynomials = Hasher::hash_elements(&polynomials);
    let challenge = (digest_polynomials[0], digest_polynomials[1]);

    let mut result: Vec<Felt> = vec![challenge.0, challenge.1];
    result.extend_from_slice(&polynomials);
    result.extend_from_slice(&nonce.to_elements());

    result.reverse();
    result
}

/// Converts a ECDSA [ecdsa_k256_keccak::Signature] to a vector of values to be pushed to be
/// written to memory. The values are the ones required for a ECDSA signature precompile inside
/// the Miden VM.
fn prepare_ecdsa_k256_keccak_signature(sig: &ecdsa_k256_keccak::Signature) -> Vec<Felt> {
    const BYTES_PER_U32: usize = size_of::<u32>();

    let bytes = sig.to_bytes();
    bytes
        .chunks(BYTES_PER_U32)
        .map(|chunk| {
            // Pack up to 4 bytes into a u32 in little-endian format
            let mut packed = [0u8; BYTES_PER_U32];
            packed[..chunk.len()].copy_from_slice(chunk);
            Felt::from(u32::from_le_bytes(packed))
        })
        .collect()
}
