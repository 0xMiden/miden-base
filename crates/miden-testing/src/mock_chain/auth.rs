// AUTH
// ================================================================================================
use alloc::vec::Vec;

use miden_lib::{
    account::auth::{AuthNone, RpoFalcon512, RpoFalcon512ProcedureAcl},
    transaction::TransactionKernel,
};
use miden_objects::{
    Digest,
    account::{AccountComponent, AuthSecretKey},
    crypto::dsa::rpo_falcon512::SecretKey,
    testing::account_component::{ConditionalAuthComponent, NoopAuthComponent},
};
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Specifies which authentication mechanism is desired for accounts
#[derive(Debug, Clone)]
pub enum Auth {
    /// Creates a [SecretKey] for the account and creates a [BasicAuthenticator] used to
    /// authenticate the account with [RpoFalcon512] from miden-lib.
    BasicAuth,

    /// Creates a [SecretKey] for the account, and creates a [BasicAuthenticator] used to
    /// authenticate the account with [RpoFalcon512ProcedureAcl] from miden-lib. Authentication will
    /// only be triggered if any of the procedures specified in the list are called during
    /// execution.
    ProcedureAcl { auth_trigger_procedures: Vec<Digest> },

    /// Creates a mock authentication mechanism for the account that does nothing.
    Noop,

    /// Creates an AuthNone component from miden-lib that only increments the nonce.
    /// This is the standard AuthNone component that should be used for testing.
    NoAuth,

    /// TODO update once #1501 is ready.
    Conditional,
}

impl Auth {
    /// Converts `self` into its corresponding authentication [`AccountComponent`] and an optional
    /// [`BasicAuthenticator`]. The component is always returned, but the authenticator is only
    /// `Some` when [`Auth::BasicAuth`] is passed."
    pub fn build_component(&self) -> (AccountComponent, Option<BasicAuthenticator<ChaCha20Rng>>) {
        match self {
            Auth::BasicAuth => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = SecretKey::with_rng(&mut rng);
                let pub_key = sec_key.public_key();

                let component = RpoFalcon512::new(pub_key).into();
                let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
                    &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
                    rng,
                );

                (component, Some(authenticator))
            },
            Auth::ProcedureAcl { auth_trigger_procedures } => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = SecretKey::with_rng(&mut rng);
                let pub_key = sec_key.public_key();

                let component =
                    RpoFalcon512ProcedureAcl::new(pub_key, auth_trigger_procedures.clone())
                        .expect("component creation failed")
                        .into();
                let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
                    &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
                    rng,
                );

                (component, Some(authenticator))
            },
            Auth::Noop => {
                let assembler = TransactionKernel::assembler();
                let component = NoopAuthComponent::new(assembler).unwrap();
                (component.into(), None)
            },

            Auth::NoAuth => {
                let component = AuthNone.into();
                (component, None)
            },

            Auth::Conditional => {
                let assembler = TransactionKernel::assembler();
                let component = ConditionalAuthComponent::new(assembler).unwrap();
                (component.into(), None)
            },
        }
    }
}

impl From<Auth> for AccountComponent {
    fn from(auth: Auth) -> Self {
        let (component, _) = auth.build_component();
        component
    }
}
