use alloc::{boxed::Box, collections::BTreeMap, string::ToString, sync::Arc, vec::Vec};

use miden_objects::{
    Felt, Hasher, Word, account::AuthSecretKey, crypto::SequentialCommit,
    transaction::TransactionSummary, utils::sync::RwLock,
};
use rand::Rng;

use super::signatures::get_falcon_signature;
use crate::errors::AuthenticationError;

// SIGNATURE DATA
// ================================================================================================

/// Data types on which a signature can be requested.
///
/// It supports three modes:
/// - `TransactionSummary`: Structured transaction summary, recommended for authenticating
///   transactions.
/// - `Arbitrary`: Arbitrary payload provided by the application. It is up to the authenticator to
///   display it appropriately.
/// - `Blind`: The underlying data is not meant to be displayed in a human-readable format. It must
///   be a cryptographic commitment to some data.
pub enum SignatureData {
    TransactionSummary(Box<TransactionSummary>),
    Arbitrary(Vec<Felt>),
    Blind(Word),
}

impl SequentialCommit for SignatureData {
    type Commitment = Word;

    fn to_elements(&self) -> Vec<Felt> {
        match self {
            SignatureData::TransactionSummary(tx_summary) => tx_summary.as_ref().to_elements(),
            SignatureData::Arbitrary(elements) => elements.clone(),
            SignatureData::Blind(word) => word.as_elements().to_vec(),
        }
    }

    fn to_commitment(&self) -> Self::Commitment {
        match self {
            // `TransactionSummary` knows how to derive a commitment to itself.
            SignatureData::TransactionSummary(tx_summary) => tx_summary.as_ref().to_commitment(),
            // use the default implementation.
            SignatureData::Arbitrary(elements) => Hasher::hash_elements(elements),
            // `Blind` is assumed to already be a commitment.
            SignatureData::Blind(word) => *word,
        }
    }
}

// TRANSACTION AUTHENTICATOR
// ================================================================================================

/// Defines an authenticator for transactions.
///
/// The main purpose of the authenticator is to generate signatures for a given message against
/// a key managed by the authenticator. That is, the authenticator maintains a set of public-
/// private key pairs, and can be requested to generate signatures against any of the managed keys.
///
/// The public keys are defined by [Word]'s which are the hashes of the actual public keys.
pub trait TransactionAuthenticator {
    /// Retrieves a signature for a specific message as a list of [Felt].
    ///
    /// The request is initiated by the VM as a consequence of the SigToStack advice
    /// injector.
    ///
    /// - `pub_key_hash`: The hash of the public key used for signature generation.
    /// - `message`: The message to sign, usually a commitment to the transaction data.
    /// - `account_delta`: An informational parameter describing the changes made to the account up
    ///   to the point of calling `get_signature()`. This allows the authenticator to review any
    ///   alterations to the account prior to signing. It should not be directly used in the
    ///   signature computation.
    fn get_signature(
        &self,
        pub_key: Word,
        signature_data: &SignatureData,
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
        signature_data: &SignatureData,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        TransactionAuthenticator::get_signature(*self, pub_key, signature_data)
    }
}

// BASIC AUTHENTICATOR
// ================================================================================================

/// Represents a signer for [AuthSecretKey] keys.
#[derive(Clone, Debug)]
pub struct BasicAuthenticator<R> {
    /// pub_key |-> secret_key mapping
    keys: BTreeMap<Word, AuthSecretKey>,
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
            key_map.insert(*word, secret_key.clone());
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
        signature_data: &SignatureData,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        let message = signature_data.to_commitment();

        let mut rng = self.rng.write();

        match self.keys.get(&pub_key) {
            Some(key) => match key {
                AuthSecretKey::RpoFalcon512(falcon_key) => {
                    get_falcon_signature(falcon_key, message, &mut *rng)
                },
            },
            None => Err(AuthenticationError::UnknownPublicKey(format!(
                "public key {pub_key} is not contained in the authenticator's keys",
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
        _signature_data: &SignatureData,
    ) -> Result<Vec<Felt>, AuthenticationError> {
        Err(AuthenticationError::RejectedSignature(
            "default authenticator cannot provide signatures".to_string(),
        ))
    }
}

#[cfg(test)]
mod test {
    use miden_lib::utils::{Deserializable, Serializable};
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
