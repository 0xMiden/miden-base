use alloc::{collections::BTreeMap, vec::Vec};

use anyhow::Context;
use miden_lib::{
    account::{faucets::BasicFungibleFaucet, wallets::BasicWallet},
    note::{create_p2id_note, create_p2ide_note},
    transaction::{TransactionKernel, memory},
};
use miden_objects::{
    Felt, NoteError, ZERO,
    account::{Account, AccountBuilder, AccountId, AccountStorageMode, AccountType, StorageSlot},
    asset::{Asset, TokenSymbol},
    block::BlockNumber,
    note::{Note, NoteType},
    testing::account_component::AccountMockComponent,
    transaction::OutputNote,
};
use rand::Rng;
use vm_processor::crypto::RpoRandomCoin;

use crate::{
    AccountState, Auth, MockChain,
    mock_chain::chain::{AccountCredentials, create_genesis_state},
    utils::create_p2any_note,
};

/// A builder for a [`MockChain`].
#[derive(Debug, Clone)]
pub struct MockChainBuilder {
    accounts: BTreeMap<AccountId, Account>,
    account_credentials: BTreeMap<AccountId, AccountCredentials>,
    notes: Vec<OutputNote>,
    rng: RpoRandomCoin,
}

impl MockChainBuilder {
    /// Inititalizes a new mock chain builder with an empty state.
    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
            account_credentials: BTreeMap::new(),
            notes: Vec::new(),
            rng: RpoRandomCoin::new(Default::default()),
        }
    }

    /// Creates a new public [`BasicWallet`] account and registers the authenticator (if any) and
    /// seed.
    ///
    /// This does not add the account to the chain state, but it can still be used to call
    /// [`MockChain::build_tx_context`] to automatically handle the authenticator and seed.
    pub fn create_new_wallet(&mut self, auth_method: Auth) -> anyhow::Result<Account> {
        let account_builder = AccountBuilder::new(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(BasicWallet);

        self.add_account_from_builder(auth_method, account_builder, AccountState::New)
    }

    /// Adds an existing public [`BasicWallet`] account to the initial chain state and registers the
    /// authenticator (if any).
    pub fn add_existing_wallet(&mut self, auth_method: Auth) -> anyhow::Result<Account> {
        self.add_existing_wallet_with_assets(auth_method, [])
    }

    /// Adds an existing public [`BasicWallet`] account to the initial chain state and registers the
    /// authenticator (if any).
    pub fn add_existing_wallet_with_assets(
        &mut self,
        auth_method: Auth,
        assets: impl IntoIterator<Item = Asset>,
    ) -> anyhow::Result<Account> {
        let account_builder = Account::builder(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(BasicWallet)
            .with_assets(assets);

        self.add_account_from_builder(auth_method, account_builder, AccountState::Exists)
    }

    /// Creates a new public [`BasicFungibleFaucet`] account and registers the authenticator (if
    /// any) and seed.
    ///
    /// This does not add the account to the chain state, but it can still be used to call
    /// [`MockChain::build_tx_context`] to automatically handle the authenticator and seed.
    pub fn create_new_faucet(
        &mut self,
        auth_method: Auth,
        token_symbol: &str,
        max_supply: u64,
    ) -> anyhow::Result<Account> {
        let token_symbol = TokenSymbol::new(token_symbol)
            .with_context(|| format!("invalid token symbol: {token_symbol}"))?;
        let max_supply_felt = max_supply.try_into().map_err(|_| {
            anyhow::anyhow!("max supply value cannot be converted to Felt: {max_supply}")
        })?;
        let basic_faucet = BasicFungibleFaucet::new(token_symbol, 10, max_supply_felt)
            .context("failed to create BasicFungibleFaucet")?;

        let account_builder = AccountBuilder::new(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .account_type(AccountType::FungibleFaucet)
            .with_component(basic_faucet);

        self.add_account_from_builder(auth_method, account_builder, AccountState::New)
    }

    /// Adds an existing public [`BasicFungibleFaucet`] account to the initial chain state and
    /// registers the authenticator (if the given [`Auth`] results in the creation of one).
    pub fn add_existing_faucet(
        &mut self,
        auth_method: Auth,
        token_symbol: &str,
        max_supply: u64,
        total_issuance: Option<u64>,
    ) -> anyhow::Result<Account> {
        let token_symbol = TokenSymbol::new(token_symbol).context("invalid argument")?;
        let basic_faucet = BasicFungibleFaucet::new(token_symbol, 10u8, Felt::new(max_supply))
            .context("invalid argument")?;

        let account_builder = AccountBuilder::new(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(basic_faucet)
            .account_type(AccountType::FungibleFaucet);

        let mut account =
            self.add_account_from_builder(auth_method, account_builder, AccountState::Exists)?;

        // The faucet's reserved slot is initialized to an empty word by default.
        // If total_issuance is set, overwrite it and reinsert the account.
        if let Some(issuance) = total_issuance {
            account
                .storage_mut()
                .set_item(memory::FAUCET_STORAGE_DATA_SLOT, [ZERO, ZERO, ZERO, Felt::new(issuance)])
                .context("failed to set faucet storage")?;
            self.accounts.insert(account.id(), account.clone());
        }

        Ok(account)
    }

    /// Creates a new public account with an [`AccountMockComponent`] and registers the
    /// authenticator (if any).
    pub fn create_new_mock_account(&mut self, auth_method: Auth) -> anyhow::Result<Account> {
        let account_builder = Account::builder(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(
                AccountMockComponent::new_with_empty_slots(TransactionKernel::assembler())
                    .context("failed to create mock component")?,
            );

        self.add_account_from_builder(auth_method, account_builder, AccountState::New)
    }

    /// Adds an existing public account with an [`AccountMockComponent`] to the initial chain state
    /// and registers the authenticator (if any).
    pub fn add_existing_mock_account(&mut self, auth_method: Auth) -> anyhow::Result<Account> {
        self.add_existing_mock_account_with_storage_and_assets(auth_method, [], [])
    }

    /// Adds an existing public account with an [`AccountMockComponent`] to the initial chain state
    /// and registers the authenticator (if any).
    pub fn add_existing_mock_account_with_storage(
        &mut self,
        auth_method: Auth,
        slots: impl IntoIterator<Item = StorageSlot>,
    ) -> anyhow::Result<Account> {
        self.add_existing_mock_account_with_storage_and_assets(auth_method, slots, [])
    }

    /// Adds an existing public account with an [`AccountMockComponent`] to the initial chain state
    /// and registers the authenticator (if any).
    pub fn add_existing_mock_account_with_assets(
        &mut self,
        auth_method: Auth,
        assets: impl IntoIterator<Item = Asset>,
    ) -> anyhow::Result<Account> {
        self.add_existing_mock_account_with_storage_and_assets(auth_method, [], assets)
    }

    /// Adds an existing public account with an [`AccountMockComponent`] to the initial chain state
    /// and registers the authenticator (if any).
    pub fn add_existing_mock_account_with_storage_and_assets(
        &mut self,
        auth_method: Auth,
        slots: impl IntoIterator<Item = StorageSlot>,
        assets: impl IntoIterator<Item = Asset>,
    ) -> anyhow::Result<Account> {
        let account_builder = Account::builder(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(
                AccountMockComponent::new_with_slots(
                    TransactionKernel::assembler(),
                    slots.into_iter().collect(),
                )
                .context("failed to create mock component")?,
            )
            .with_assets(assets);

        self.add_account_from_builder(auth_method, account_builder, AccountState::Exists)
    }

    /// Builds the provided [`AccountBuilder`] with the provided auth method and registers the
    /// authenticator (if any).
    ///
    /// - If [`AccountState::Exists`] is given the account is built as an existing account and added
    ///   to the initial chain state. It can then be used in a transaction without having to
    ///   validate its seed.
    /// - If [`AccountState::New`] is given the account is built as a new account and is **not**
    ///   added to the chain. Its seed and authenticator are registered (if any). Its first
    ///   transaction will be its creation transaction. [`MockChain::build_tx_context`] can be
    ///   called with the account to automatically handle the authenticator and seed.
    pub fn add_account_from_builder(
        &mut self,
        auth_method: Auth,
        mut account_builder: AccountBuilder,
        account_state: AccountState,
    ) -> anyhow::Result<Account> {
        let (auth_component, authenticator) = auth_method.build_component();
        account_builder = account_builder.with_auth_component(auth_component);

        let (account, seed) = if let AccountState::New = account_state {
            let (account, seed) =
                account_builder.build().context("failed to build account from builder")?;
            (account, Some(seed))
        } else {
            let account = account_builder
                .build_existing()
                .context("failed to build account from builder")?;
            (account, None)
        };

        self.account_credentials
            .insert(account.id(), AccountCredentials::new(seed, authenticator));

        if let AccountState::Exists = account_state {
            self.accounts.insert(account.id(), account.clone());
        }

        Ok(account)
    }

    /// Adds the provided account to the list of genesis accounts.
    ///
    /// This method only adds the account and cannot not register any seed or authenticator for it.
    /// Calling [`MockChain::build_tx_context`] on accounts added in this way will not work if the
    /// account is new or if it needs an authenticator.
    ///
    /// Due to these limitations, prefer using other methods to add accounts to the chain, e.g.
    /// [`MockChainBuilder::add_account_from_builder`].
    pub fn add_account(&mut self, account: Account) -> anyhow::Result<()> {
        self.accounts.insert(account.id(), account);

        // This returns a Result to be conservative in case we need to return an error in the future
        // and do not want to break this API.
        Ok(())
    }

    /// Adds the provided note to the initial chain state.
    pub fn add_note(&mut self, note: impl Into<OutputNote>) {
        self.notes.push(note.into());
    }

    /// Creates a new P2ANY note from the provided parameters and adds it to the list of genesis
    /// notes. This note is similar to a P2ID note but can be consumed by any account.
    ///
    /// In the created [`MockChain`], the note will be immediately spendable by `target_account_id`
    /// and carries no additional reclaim or timelock conditions.
    pub fn add_p2any_note(
        &mut self,
        sender_account_id: AccountId,
        asset: &[Asset],
    ) -> anyhow::Result<Note> {
        let note = create_p2any_note(sender_account_id, asset);

        self.add_note(OutputNote::Full(note.clone()));

        Ok(note)
    }

    /// Creates a new P2ID note from the provided parameters and adds it to the list of genesis
    /// notes.
    ///
    /// In the created [`MockChain`], the note will be immediately spendable by `target_account_id`
    /// and carries no additional reclaim or timelock conditions.
    pub fn add_p2id_note(
        &mut self,
        sender_account_id: AccountId,
        target_account_id: AccountId,
        asset: &[Asset],
        note_type: NoteType,
    ) -> Result<Note, NoteError> {
        let note = create_p2id_note(
            sender_account_id,
            target_account_id,
            asset.to_vec(),
            note_type,
            Default::default(),
            &mut self.rng,
        )?;

        self.add_note(OutputNote::Full(note.clone()));

        Ok(note)
    }

    /// Adds a P2IDE [`OutputNote`] (pay‑to‑ID‑extended) to the list of genesis notes.
    ///
    /// A P2IDE note can include an optional `timelock_height` and/or an optional
    /// `reclaim_height` after which the `sender_account_id` may reclaim the
    /// funds.
    pub fn add_pending_p2ide_note(
        &mut self,
        sender_account_id: AccountId,
        target_account_id: AccountId,
        asset: &[Asset],
        note_type: NoteType,
        reclaim_height: Option<BlockNumber>,
        timelock_height: Option<BlockNumber>,
    ) -> Result<Note, NoteError> {
        let note = create_p2ide_note(
            sender_account_id,
            target_account_id,
            asset.to_vec(),
            reclaim_height,
            timelock_height,
            note_type,
            Default::default(),
            &mut self.rng,
        )?;

        self.add_note(OutputNote::Full(note.clone()));

        Ok(note)
    }

    /// Consumes the builder, creates the genesis block of the chain and returns the [`MockChain`].
    pub fn build(self) -> anyhow::Result<MockChain> {
        let (genesis_block, account_tree) =
            create_genesis_state(self.accounts.into_values(), self.notes)
                .context("failed to create genesis block")?;

        MockChain::from_genesis_block(genesis_block, account_tree, self.account_credentials)
    }
}

impl Default for MockChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}
