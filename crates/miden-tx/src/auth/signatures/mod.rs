use alloc::vec::Vec;

use miden_objects::Hasher;
use miden_objects::crypto::dsa::rpo_falcon512::{self, Polynomial};
use miden_processor::{Felt, Word};
use rand::Rng;

use crate::AuthenticationError;

pub enum Signature {
    RpoFalcon512(rpo_falcon512::Signature),
}

impl Signature {
    pub fn to_prepared_signature(&self) -> Result<Vec<Felt>, AuthenticationError> {
        match self {
            Signature::RpoFalcon512(signature) => prepare_rpo_falcon512_signature(signature),
        }
    }
}

fn prepare_rpo_falcon512_signature(
    sig: &rpo_falcon512::Signature,
) -> Result<Vec<Felt>, AuthenticationError> {
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
    Ok(result)
}

/// Retrieves a Falcon signature over a message.
///
/// Gets as input a [Word] containing a secret key, and a [Word] representing a message and
/// outputs a vector of values to be pushed onto the advice stack. The values are the ones required
/// for a Falcon signature verification inside the VM and they are:
///
/// 1. The challenge point at which we evaluate the polynomials in the subsequent three bullet
///    points, i.e. `h`, `s2` and `pi`, to check the product relationship.
/// 2. The expanded public key represented as the coefficients of a polynomial `h` of degree < 512.
/// 3. The signature represented as the coefficients of a polynomial `s2` of degree < 512.
/// 4. The product of the above two polynomials `pi` in the ring of polynomials with coefficients in
///    the Miden field.
/// 5. The nonce represented as 8 field elements.
///
/// # Errors
/// Will return an error if either:
/// - The secret key is malformed due to either incorrect length or failed decoding.
/// - The signature generation failed.
pub fn get_falcon_signature<R: Rng>(
    key: &rpo_falcon512::SecretKey,
    message: Word,
    rng: &mut R,
) -> Result<Vec<Felt>, AuthenticationError> {
    // Generate the signature
    let sig = key.sign_with_rng(message, rng);

    prepare_rpo_falcon512_signature(&sig)
}
