// AUTH
// ================================================================================================
use alloc::vec::Vec;

use miden_lib::{
    account::auth::{RpoFalcon512, RpoFalcon512ProcedureAcl},
    transaction::TransactionKernel,
};
use miden_objects::{
    Digest,
    account::{AccountComponent, AuthSecretKey},
    crypto::dsa::rpo_falcon512::SecretKey,
    testing::account_component::{
        ConditionalAuthComponent, IncrNonceAuthComponent, NoopAuthComponent,
    },
};
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Specifies which authentication mechanism is desired for accounts
#[derive(Debug, Clone)]
pub enum Auth {
    /// Creates a [SecretKey] for the account and creates a [BasicAuthenticator] used to
    /// authenticate the account with the [RpoFalcon512].
    BasicAuth,

    /// Creates a [SecretKey] for the account, and creates a [BasicAuthenticator] used to
    /// authenticate the account with the [RpoFalcon512ProcedureAcl]. Authentication will only be
    /// triggered if any of the procedures specified in the list are called.
    ///
    /// # Panics
    /// Panics if more than 256 procedures are tracked.
    ProcedureAcl { trigger_procedures: Vec<Digest> },

    /// Creates a mock authentication mechanism for the account that only increments the nonce.
    IncrNonce,

    /// Creates a mock authentication mechanism for the account that does nothing.
    Noop,

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
            Auth::ProcedureAcl { trigger_procedures } => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = SecretKey::with_rng(&mut rng);
                let pub_key = sec_key.public_key();

                let component =
                    RpoFalcon512ProcedureAcl::new(pub_key, trigger_procedures.clone()).into();
                let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
                    &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
                    rng,
                );

                (component, Some(authenticator))
            },
            Auth::IncrNonce => {
                let assembler = TransactionKernel::assembler();
                let component = IncrNonceAuthComponent::new(assembler).unwrap();
                (component.into(), None)
            },

            Auth::Noop => {
                let assembler = TransactionKernel::assembler();
                let component = NoopAuthComponent::new(assembler).unwrap();
                (component.into(), None)
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
