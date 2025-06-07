use miden_objects::{
    account::{Account, AccountId},
    asset::{Asset, FungibleAsset},
};

// MOCK FUNGIBLE FAUCET
// ================================================================================================

/// Represents a fungible faucet that exists on the MockChain.
pub struct MockFungibleFaucet(Account);

impl MockFungibleFaucet {
    pub(super) fn new(account: Account) -> Self {
        Self(account)
    }

    pub fn account(&self) -> &Account {
        &self.0
    }

    pub fn id(&self) -> AccountId {
        self.0.id()
    }

    pub fn mint(&self, amount: u64) -> Asset {
        FungibleAsset::new(self.0.id(), amount).unwrap().into()
    }
}
