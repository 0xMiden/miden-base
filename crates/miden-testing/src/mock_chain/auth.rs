// AUTH
// ================================================================================================
use alloc::vec::Vec;

use miden_lib::account::auth::{
    AuthRpoFalcon512,
    AuthRpoFalcon512Acl,
    AuthRpoFalcon512AclConfig,
    AuthRpoFalcon512Multisig,
    AuthRpoFalcon512MultisigConfig,
};
use miden_lib::testing::account_component::{ConditionalAuthComponent, IncrNonceAuthComponent};
use miden_objects::Word;
use miden_objects::account::AccountComponent;
use miden_objects::account::auth::{AuthSecretKey, PublicKeyCommitment};
use miden_objects::testing::noop_auth_component::NoopAuthComponent;
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Specifies which authentication mechanism is desired for accounts
#[derive(Debug, Clone)]
pub enum Auth {
    /// Creates a secret key for the account and creates a [BasicAuthenticator] used to
    /// authenticate the account with [AuthRpoFalcon512].
    BasicAuth,

    /// Multisig
    Multisig {
        threshold: u32,
        approvers: Vec<Word>,
        proc_threshold_map: Vec<(Word, u32)>,
    },

    /// Creates a secret key for the account, and creates a [BasicAuthenticator] used to
    /// authenticate the account with [AuthRpoFalcon512Acl]. Authentication will only be
    /// triggered if any of the procedures specified in the list are called during execution.
    Acl {
        auth_trigger_procedures: Vec<Word>,
        allow_unauthorized_output_notes: bool,
        allow_unauthorized_input_notes: bool,
    },

    /// Creates a mock authentication mechanism for the account that only increments the nonce.
    IncrNonce,

    /// Creates a mock authentication mechanism for the account that does nothing.
    Noop,

    /// Creates a mock authentication mechanism for the account that conditionally succeeds and
    /// conditionally increments the nonce based on the authentication arguments.
    ///
    /// The auth procedure expects the first three arguments as [99, 98, 97] to succeed.
    /// In case it succeeds, it conditionally increments the nonce based on the fourth argument.
    Conditional,
}

impl Auth {
    /// Converts `self` into its corresponding authentication [`AccountComponent`] and an optional
    /// [`BasicAuthenticator`]. The component is always returned, but the authenticator is only
    /// `Some` when [`Auth::BasicAuth`] is passed."
    pub fn build_component(&self) -> (AccountComponent, Option<BasicAuthenticator>) {
        match self {
            Auth::BasicAuth => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = AuthSecretKey::new_rpo_falcon512_with_rng(&mut rng);
                let pub_key = sec_key.public_key().to_commitment();

                let component = AuthRpoFalcon512::new(pub_key).into();
                let authenticator = BasicAuthenticator::new(&[sec_key]);

                (component, Some(authenticator))
            },
            Auth::Multisig { threshold, approvers, proc_threshold_map } => {
                let pub_keys: Vec<_> =
                    approvers.iter().map(|word| PublicKeyCommitment::from(*word)).collect();

                let config = AuthRpoFalcon512MultisigConfig::new(pub_keys, *threshold)
                    .and_then(|cfg| cfg.with_proc_thresholds(proc_threshold_map.clone()))
                    .expect("invalid multisig config");
                let component = AuthRpoFalcon512Multisig::new(config)
                    .expect("multisig component creation failed")
                    .into();

                (component, None)
            },
            Auth::Acl {
                auth_trigger_procedures,
                allow_unauthorized_output_notes,
                allow_unauthorized_input_notes,
            } => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = AuthSecretKey::new_rpo_falcon512_with_rng(&mut rng);
                let pub_key = sec_key.public_key().to_commitment();

                let component = AuthRpoFalcon512Acl::new(
                    pub_key,
                    AuthRpoFalcon512AclConfig::new()
                        .with_auth_trigger_procedures(auth_trigger_procedures.clone())
                        .with_allow_unauthorized_output_notes(*allow_unauthorized_output_notes)
                        .with_allow_unauthorized_input_notes(*allow_unauthorized_input_notes),
                )
                .expect("component creation failed")
                .into();
                let authenticator = BasicAuthenticator::new(&[sec_key]);

                (component, Some(authenticator))
            },
            Auth::IncrNonce => (IncrNonceAuthComponent.into(), None),
            Auth::Noop => (NoopAuthComponent.into(), None),
            Auth::Conditional => (ConditionalAuthComponent.into(), None),
        }
    }
}

impl From<Auth> for AccountComponent {
    fn from(auth: Auth) -> Self {
        let (component, _) = auth.build_component();
        component
    }
}
