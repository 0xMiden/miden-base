use miden_objects::account::AccountComponent;

use crate::account::components::no_auth_library;

/// An [`AccountComponent`] implementing a no-authentication scheme.
///
/// This component provides **no authentication**! It only checks if the account
/// state has actually changed during transaction execution by comparing the initial
/// account commitment with the current commitment and increments the nonce if
/// they differ. This avoids unnecessary nonce increments for transactions that don't
/// modify the account state.
///
/// It exports the procedure `auth__no_auth`, which:
/// - Checks if the account state has changed by comparing initial and final commitments
/// - Only increments the nonce if the account state has actually changed
/// - Provides no cryptographic authentication
///
/// This component supports all account types.
pub struct NoAuth;

impl NoAuth {
    /// Creates a new [`NoAuth`] component.
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl From<NoAuth> for AccountComponent {
    fn from(_: NoAuth) -> Self {
        AccountComponent::new(no_auth_library(), vec![])
            .expect("NoAuth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

#[cfg(test)]
mod tests {
    use miden_objects::account::AccountBuilder;

    use super::*;
    use crate::account::wallets::BasicWallet;

    #[test]
    fn test_no_auth_component() {
        // Create an account using the NoAuth component
        let (_account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(NoAuth)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");
    }
}
