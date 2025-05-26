use miden_objects::{
    AccountError,
    account::{Account, AccountDelta},
};
use miden_tx::auth::BasicAuthenticator;
use rand_chacha::ChaCha20Rng;
use vm_processor::Word;

// MOCK ACCOUNT
// ================================================================================================

/// Represents a mock account that exists on the MockChain.
/// It optionally includes the seed, and an authenticator that can be used to authenticate
/// transaction contexts.
#[derive(Clone, Debug)]
pub(super) struct MockAccount {
    account: Account,
    seed: Option<Word>,
    authenticator: Option<BasicAuthenticator<ChaCha20Rng>>,
}

impl MockAccount {
    pub(super) fn new(
        account: Account,
        seed: Option<Word>,
        authenticator: Option<BasicAuthenticator<ChaCha20Rng>>,
    ) -> Self {
        MockAccount { account, seed, authenticator }
    }

    pub fn apply_delta(&mut self, delta: &AccountDelta) -> Result<(), AccountError> {
        self.account.apply_delta(delta)
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn seed(&self) -> Option<&Word> {
        self.seed.as_ref()
    }

    pub fn authenticator(&self) -> Option<&BasicAuthenticator<ChaCha20Rng>> {
        self.authenticator.as_ref()
    }
}
