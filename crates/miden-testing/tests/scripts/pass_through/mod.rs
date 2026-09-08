//! Tests for the canonical pass-through transaction scripts, one module per shape of output the
//! assets are forwarded into.

use miden_protocol::account::auth::AuthScheme;
use miden_protocol::account::{Account, AccountBuilder, AccountType};
use miden_protocol::asset::Asset;
use miden_standards::account::pass_through::PassThroughSweep;
use miden_standards::account::wallets::BasicWallet;
use miden_testing::{AccountState, Auth, MockChainBuilder};

mod single_p2id;

/// The signature scheme the pass-through accounts in these tests authenticate with. ECDSA verifies
/// far faster than Falcon, and none of these tests are about the scheme itself.
pub(crate) const AUTH_SCHEME: AuthScheme = AuthScheme::EcdsaK256Keccak;

/// The components of the immutable account a pass-through transaction runs on: `AuthPassThrough`
/// so that only the key holder can transact and any change to the account fails, `BasicWallet` so
/// input notes can deposit into it, and `PassThroughSweep` for the account procedure the scripts
/// call.
fn pass_through_account_builder(
    seed: [u8; 32],
    assets: impl IntoIterator<Item = Asset>,
) -> AccountBuilder {
    AccountBuilder::new(seed)
        .with_component(BasicWallet)
        .with_component(PassThroughSweep)
        .with_assets(assets)
        .account_type(AccountType::Public)
}

/// Builds a pass-through account without registering it anywhere, for tests that only need its
/// [`AccountCodeInterface`](miden_protocol::account::AccountCodeInterface) and execute nothing.
pub(crate) fn pass_through_account() -> anyhow::Result<Account> {
    let (auth_components, _) = Auth::PassThrough { auth_scheme: AUTH_SCHEME }.build_components();

    Ok(pass_through_account_builder([42; 32], [])
        .with_components(auth_components)
        .build_existing()?)
}

/// Adds an existing pass-through account to the chain.
///
/// Registers it through the builder rather than with `add_account`, so the chain also learns the
/// authenticator that signs for it - without which every transaction fails as unauthenticated.
pub(crate) fn add_pass_through_account(builder: &mut MockChainBuilder) -> anyhow::Result<Account> {
    add_pass_through_account_with(builder, [42; 32], [], AccountState::Exists)
}

/// As [`add_pass_through_account`], but lets the caller pick the account seed, any assets it
/// already holds, and whether it exists or is created by the transaction under test.
pub(crate) fn add_pass_through_account_with(
    builder: &mut MockChainBuilder,
    seed: [u8; 32],
    assets: impl IntoIterator<Item = Asset>,
    state: AccountState,
) -> anyhow::Result<Account> {
    builder.add_account_from_builder(
        Auth::PassThrough { auth_scheme: AUTH_SCHEME },
        pass_through_account_builder(seed, assets),
        state,
    )
}
