use alloc::{collections::BTreeMap, string::ToString, sync::Arc, vec::Vec};

use miden_lib::objects_utils::sync::RwLock;
use miden_objects::account::{AccountDelta, AuthSecretKey};
use rand::Rng;
use vm_processor::{Digest, Felt, Word};

use super::signatures::get_falcon_signature;
use crate::errors::AuthenticationError;

// TRANSACTION AUTHENTICATOR
// ================================================================================================

/// Defines an authenticator for transactions.
///
/// The main purpose of the authenticator is to generate signatures for a given message against
/// a key managed by the authenticator. That is, the authenticator maintains a set of public-
/// private key pairs, and can be requested to generate signatures against any of the managed keys.
///
/// The public keys are defined by [Digest]'s which are the hashes of the actual public keys.
pub trait TransactionAuthenticator {
    /// Retrieves a signature for a specific message as a list of [Felt].
    ///
    /// The request is initiated by the VM as a consequence of the SigToStack advice
    /// injector.
    ///
    /// - `pub_key`: The public key used for signature generation.
    /// - `message`: The message to sign, usually a commitment to the transaction data.
    /// - `account_delta`: An informational parameter describing the changes made to the account up
    ///   to the point of calling `get_signature()`. This allows the authenticator to review any
    ///   alterations to the account prior to signing. It should not be directly used in the
    ///   signature computation.
    fn get_signature(
        &self,
        pub_key: Word,
        message: Word,
        account_delta: &AccountDelta,
    ) -> Result<Vec<Felt>, AuthenticationError>;
}

/// This blanket implementation is required to allow `Option<&T>` to be mapped to `Option<&dyn
/// TransactionAuthenticator`>.
impl<T> TransactionAuthenticator for &T
where
    T: TransactionAuthenticator + ?Sized,
{
    fn get_signature(
        &self,
        pub_key: Word,
        message: Word,
        account_delta: &AccountDelta,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        TransactionAuthenticator::get_signature(*self, pub_key, message, account_delta)
    }
}

// BASIC AUTHENTICATOR
// ================================================================================================

#[derive(Clone, Debug)]
/// Represents a signer for [AuthSecretKey] keys.
pub struct BasicAuthenticator<R> {
    /// pub_key |-> secret_key mapping
    keys: BTreeMap<Digest, AuthSecretKey>,
    rng: Arc<RwLock<R>>,
}

impl<R: Rng> BasicAuthenticator<R> {
    #[cfg(feature = "std")]
    pub fn new(keys: &[(Word, AuthSecretKey)]) -> BasicAuthenticator<rand::rngs::StdRng> {
        use rand::{SeedableRng, rngs::StdRng};

        let rng = StdRng::from_os_rng();
        BasicAuthenticator::<StdRng>::new_with_rng(keys, rng)
    }

    pub fn new_with_rng(keys: &[(Word, AuthSecretKey)], rng: R) -> Self {
        let mut key_map = BTreeMap::new();
        for (word, secret_key) in keys {
            key_map.insert(word.into(), secret_key.clone());
        }

        BasicAuthenticator {
            keys: key_map,
            rng: Arc::new(RwLock::new(rng)),
        }
    }
}

impl<R: Rng> TransactionAuthenticator for BasicAuthenticator<R> {
    /// Gets a signature over a message, given a public key.
    /// The key should be included in the `keys` map and should be a variant of [AuthSecretKey].
    ///
    /// Supported signature schemes:
    /// - RpoFalcon512
    ///
    /// # Errors
    /// If the public key is not contained in the `keys` map,
    /// [`AuthenticationError::UnknownPublicKey`] is returned.
    fn get_signature(
        &self,
        pub_key: Word,
        message: Word,
        account_delta: &AccountDelta,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        let _ = account_delta;
        let mut rng = self.rng.write();

        match self.keys.get(&pub_key.into()) {
            Some(key) => match key {
                AuthSecretKey::RpoFalcon512(falcon_key) => {
                    get_falcon_signature(falcon_key, message, &mut *rng)
                },
            },
            None => Err(AuthenticationError::UnknownPublicKey(format!(
                "public key {} is not contained in the authenticator's keys",
                Digest::from(pub_key)
            ))),
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

impl TransactionAuthenticator for () {
    fn get_signature(
        &self,
        _pub_key: Word,
        _message: Word,
        _account_delta: &AccountDelta,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        Err(AuthenticationError::RejectedSignature(
            "default authenticator cannot provide signatures".to_string(),
        ))
    }
}

#[cfg(test)]
mod test {
    use miden_lib::objects_utils::{Deserializable, Serializable};
    use miden_objects::{account::AuthSecretKey, crypto::dsa::rpo_falcon512::SecretKey};

    #[test]
    fn serialize_auth_key() {
        let secret_key = SecretKey::new();
        let auth_key = AuthSecretKey::RpoFalcon512(secret_key.clone());
        let serialized = auth_key.to_bytes();
        let deserialized = AuthSecretKey::read_from_bytes(&serialized).unwrap();

        match deserialized {
            AuthSecretKey::RpoFalcon512(key) => assert_eq!(secret_key.to_bytes(), key.to_bytes()),
        }
    }
}
