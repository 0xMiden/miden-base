// AUTH
// ================================================================================================
use alloc::vec::Vec;

use miden_lib::{
    account::auth::{AuthRpoFalcon512, AuthRpoFalcon512Acl, AuthRpoFalcon512AclConfig},
    transaction::TransactionKernel,
};
use miden_objects::{
    Word,
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
    /// authenticate the account with [AuthRpoFalcon512].
    BasicAuth,

    /// Creates a [SecretKey] for the account, and creates a [BasicAuthenticator] used to
    /// authenticate the account with [AuthRpoFalcon512Acl]. Authentication will only be
    /// triggered if any of the procedures specified in the list are called during execution.
    ProcedureAcl {
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
    pub fn build_component(&self) -> (AccountComponent, Option<BasicAuthenticator<ChaCha20Rng>>) {
        match self {
            Auth::BasicAuth => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = SecretKey::with_rng(&mut rng);
                let pub_key = sec_key.public_key();

                let component = AuthRpoFalcon512::new(pub_key).into();
                let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
                    &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
                    rng,
                );

                (component, Some(authenticator))
            },
            Auth::ProcedureAcl {
                auth_trigger_procedures,
                allow_unauthorized_output_notes,
                allow_unauthorized_input_notes,
            } => {
                let mut rng = ChaCha20Rng::from_seed(Default::default());
                let sec_key = SecretKey::with_rng(&mut rng);
                let pub_key = sec_key.public_key();

                let component = AuthRpoFalcon512Acl::new(
                    pub_key,
                    AuthRpoFalcon512AclConfig::new()
                        .with_auth_trigger_procedures(auth_trigger_procedures.clone())
                        .with_allow_unauthorized_output_notes(*allow_unauthorized_output_notes)
                        .with_allow_unauthorized_input_notes(*allow_unauthorized_input_notes),
                )
                .expect("component creation failed")
                .into();
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
