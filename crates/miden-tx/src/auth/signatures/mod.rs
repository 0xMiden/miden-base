use alloc::vec::Vec;

use miden_objects::Hasher;
use miden_objects::crypto::dsa::rpo_falcon512::{self, Polynomial};
use miden_processor::Felt;

/// Represents a signature object ready for native verification.
///
/// In order to use this signature within the Miden VM, a preparation step may be necessary to
/// convert the native signature into a vector of field elements that can be loaded into the advice
/// provider. To prepare the signature, use the provided `to_prepared_signature` method:
/// ```rust,no_run
/// use miden_objects::crypto::dsa::rpo_falcon512::SecretKey;
/// use miden_objects::{Felt, Word};
/// use miden_tx::auth::signatures::Signature;
///
/// let secret_key = SecretKey::new();
/// let message = Word::default();
/// let signature: Signature = secret_key.sign(message).into();
/// let prepared_signature: Vec<Felt> = signature.to_prepared_signature();
/// ```
pub enum Signature {
    RpoFalcon512(rpo_falcon512::Signature),
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
    let h = sig.pk_poly();
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
