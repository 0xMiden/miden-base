// AUTH
// ================================================================================================

use miden_crypto::dsa::rpo_falcon512::SecretKey;
use miden_lib::account::auth::RpoFalcon512;
use miden_objects::account::{AccountComponent, AuthSecretKey};
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// Specifies which authentication mechanism is desired for accounts
#[derive(Debug, Clone, Copy)]
pub enum Auth {
    /// Creates a [SecretKey] for the account and creates a [BasicAuthenticator] that gets used
    /// for authenticating the account.
    BasicAuth,

    /// Does not create any authentication mechanism for the account.
    NoAuth,
}

impl Auth {
    /// Converts `self` into its corresponding authentication [`AccountComponent`] and an optional
    /// [`BasicAuthenticator`]. The component is always returned, but the authenticator is `None`
    /// when [`Auth::NoAuth`] is passed.
    pub fn build_component(
        &self,
    ) -> (AccountComponent, Option<BasicAuthenticator<ChaCha20Rng>>) {
        let mut rng = ChaCha20Rng::from_seed(Default::default());
        let sec_key = SecretKey::with_rng(&mut rng);
        let pub_key = sec_key.public_key();

        let component = RpoFalcon512::new(pub_key).into();
        match self {
            Auth::BasicAuth => {
                let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
                    &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
                    rng,
                );

                (component, Some(authenticator))
            },
            Auth::NoAuth => (component, None),
        }
    }
}
