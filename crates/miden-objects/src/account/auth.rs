use alloc::vec::Vec;

use miden_crypto::dsa::rpo_falcon512::PublicKey as RpoFalconPublicKey;

use crate::crypto::dsa::rpo_falcon512::{self, Polynomial, SecretKey};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{Felt, Hasher, Word};

// AUTH SCHEME
// ================================================================================================

const RPO_FALCON_512: u8 = 0;

/// Defines standard authentication schemes (i.e., signature schemes) available in the Miden
/// protocol.
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum AuthScheme {
    RpoFalcon512 = RPO_FALCON_512,
}

impl AuthScheme {
    /// Returns a numerical value of this auth scheme.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

// AUTH SECRET KEY
// ================================================================================================

/// Types of secret keys used for signing messages
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum AuthSecretKey {
    RpoFalcon512(rpo_falcon512::SecretKey) = 0,
}

impl AuthSecretKey {
    /// Identifier for the type of authentication key
    pub fn auth_scheme(&self) -> AuthScheme {
        match self {
            AuthSecretKey::RpoFalcon512(_) => AuthScheme::RpoFalcon512,
        }
    }
}

impl Serializable for AuthSecretKey {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.auth_scheme().as_u8());
        match self {
            AuthSecretKey::RpoFalcon512(secret_key) => {
                secret_key.write_into(target);
            },
        }
    }
}

impl Deserializable for AuthSecretKey {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let auth_key_id: u8 = source.read_u8()?;
        match auth_key_id {
            RPO_FALCON_512 => {
                let secret_key = SecretKey::read_from(source)?;
                Ok(AuthSecretKey::RpoFalcon512(secret_key))
            },
            val => Err(DeserializationError::InvalidValue(format!("Invalid auth scheme ID {val}"))),
        }
    }
}

// PUBLIC KEY
// ================================================================================================

/// Commitment to a public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKeyCommitment(Word);

impl From<RpoFalconPublicKey> for PublicKeyCommitment {
    fn from(value: RpoFalconPublicKey) -> Self {
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
    RpoFalcon512(rpo_falcon512::Signature) = 0,
}

impl Signature {
    pub fn to_prepared_signature(&self) -> Vec<Felt> {
        match self {
            Signature::RpoFalcon512(signature) => prepare_rpo_falcon512_signature(signature),
        }
    }
}

impl From<rpo_falcon512::Signature> for Signature {
    fn from(signature: rpo_falcon512::Signature) -> Self {
        Signature::RpoFalcon512(signature)
    }
}

impl Signature {
    /// Identifier for the type of signature scheme
    pub fn signature_scheme_id(&self) -> u8 {
        match self {
            Signature::RpoFalcon512(_) => 0u8,
        }
    }
}

impl Serializable for Signature {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.signature_scheme_id());
        match self {
            Signature::RpoFalcon512(signature) => {
                signature.write_into(target);
            },
        }
    }
}

impl Deserializable for Signature {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let signature_scheme_id: u8 = source.read_u8()?;
        match signature_scheme_id {
            // RpoFalcon512
            0u8 => {
                let signature = rpo_falcon512::Signature::read_from(source)?;
                Ok(Signature::RpoFalcon512(signature))
            },
            val => Err(DeserializationError::InvalidValue(format!(
                "Invalid signature scheme ID {val}"
            ))),
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
