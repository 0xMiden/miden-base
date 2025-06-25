// AUTH
// ================================================================================================
extern crate std;

use std::sync::LazyLock;

use assembly::Library;
use miden_crypto::dsa::rpo_falcon512::SecretKey;
use miden_lib::{account::auth::RpoFalcon512, transaction::TransactionKernel};
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

    /// Creates a dummy authentication mechanism for the account.
    Mock,
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
            Auth::Mock => (MockComponent.into(), None),
        }
    }
}

const AUTH_CODE: &str = "
    use.miden::account

    export.auth
        push.1 exec.account::incr_nonce
    end
";
static AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    TransactionKernel::testing_assembler()
        .assemble_library([AUTH_CODE])
        .expect("code should be valid")
});

struct MockComponent;
impl From<MockComponent> for AccountComponent {
    fn from(_auth: MockComponent) -> Self {
        AccountComponent::new(AUTH_LIBRARY.clone(), vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
