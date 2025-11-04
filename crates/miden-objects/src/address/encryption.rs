use alloc::vec::Vec;

use crypto_box::{ChaChaBox, PublicKey as PublicEncryptionKey, SecretKey as SecretDecryptionKey};
use crypto_box::aead::{Aead, AeadCore, OsRng};
use crypto_box::aead::rand_core::RngCore;

use crate::address::Address;
use crate::AddressError;

/// Seals `plaintext` for the recipient specified by `address` using the recipient's
/// public encryption key contained in its routing parameters. Returns `None` if
/// the address does not contain an encryption key.
pub fn seal_for_address<_R: rand::Rng + rand::CryptoRng>(
    _rng: &mut _R,
    address: &Address,
    plaintext: &[u8],
) -> Option<Vec<u8>> {
    let recipient_pk: &PublicEncryptionKey = address.encryption_key()?;

    // Ephemeral sender keypair
    // Generate ephemeral keypair
    let mut sk_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut sk_bytes);
    let ephemeral_sk = SecretDecryptionKey::from_bytes(sk_bytes);
    let ephemeral_pk = PublicEncryptionKey::from(&ephemeral_sk);

    let seal_box = ChaChaBox::new(recipient_pk, &ephemeral_sk);
    let nonce = ChaChaBox::generate_nonce(&mut OsRng);

    let mut sealed = Vec::with_capacity(32 + 24 + plaintext.len() + 16);
    // prefix ephemeral public key (32 bytes)
    sealed.extend_from_slice(ephemeral_pk.as_bytes());
    // then nonce (24 bytes)
    sealed.extend_from_slice(nonce.as_slice());
    // then ciphertext
    let ciphertext = seal_box.encrypt(&nonce, plaintext).ok()?;
    sealed.extend_from_slice(&ciphertext);

    Some(sealed)
}

/// Unseals a sealed box `ciphertext` using the recipient's secret decryption key.
pub fn unseal_with_secret_key(
    secret: &SecretDecryptionKey,
    ciphertext: &[u8],
) -> Result<Vec<u8>, AddressError> {
    if ciphertext.len() < 32 + 24 {
        return Err(AddressError::decode_error("sealed box too short"));
    }

    let eph_pk_bytes: [u8; 32] = ciphertext[0..32].try_into().unwrap();
    let nonce_bytes: [u8; 24] = ciphertext[32..56].try_into().unwrap();
    let payload = &ciphertext[56..];

    let eph_pk = PublicEncryptionKey::from(eph_pk_bytes);
    let open_box = ChaChaBox::new(&eph_pk, secret);
    let nonce = *crypto_box::aead::generic_array::GenericArray::from_slice(&nonce_bytes);

    open_box
        .decrypt(&nonce, payload)
        .map_err(|_| AddressError::decode_error("failed to unseal ciphertext"))
}
