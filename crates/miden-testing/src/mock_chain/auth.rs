// AUTH
// ================================================================================================
use miden_crypto::dsa::rpo_falcon512::SecretKey;
use miden_lib::{account::auth::RpoFalcon512, transaction::TransactionKernel};
use miden_objects::{
    account::{AccountComponent, AuthSecretKey},
    testing::account_component::{ConditionalAuthComponent, MockAuthComponent, NoopAuthComponent},
};
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Specifies which authentication mechanism is desired for accounts
#[derive(Debug, Clone, Copy)]
pub enum Auth {
    /// Creates a [SecretKey] for the account and creates a [BasicAuthenticator] that gets used
    /// for authenticating the account.
    BasicAuth,

    /// Creates a mock authentication mechanism for the account that only increments the nonce.
    Mock,

    /// Creates a mock authentication mechanism for the account that does nothing.
    Noop,

    /// Creates a mock authentication mechanism for the account that does nothing if state hasn't
    /// changed, and increments the nonce otherwise.
    Conditional,
}

impl Auth {
    /// Converts `self` into its corresponding authentication [`AccountComponent`] and an optional
    /// [`BasicAuthenticator`]. The component is always returned, but the authenticator is `None`
    /// when [`Auth::Mock`] is passed.
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
            Auth::Mock => {
                let assembler = TransactionKernel::testing_assembler();
                let component = MockAuthComponent::from_assembler(assembler).unwrap();
                (component.into(), None)
            },

            Auth::Noop => {
                let assembler = TransactionKernel::testing_assembler();
                let component = NoopAuthComponent::from_assembler(assembler).unwrap();
                (component.into(), None)
            },
            Auth::Conditional => {
                let assembler = TransactionKernel::testing_assembler();
                let component = ConditionalAuthComponent::from_assembler(assembler).unwrap();
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
