use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use anyhow::Context;
use itertools::Itertools;
use miden_lib::account::faucets::BasicFungibleFaucet;
use miden_lib::account::wallets::BasicWallet;
use miden_lib::note::{create_p2id_note, create_p2ide_note, create_swap_note};
use miden_lib::testing::account_component::AccountMockComponent;
use miden_lib::transaction::{TransactionKernel, memory};
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountId,
    AccountStorageMode,
    AccountType,
    StorageSlot,
};
use miden_objects::asset::{Asset, FungibleAsset, TokenSymbol};
use miden_objects::block::{
    AccountTree,
    BlockAccountUpdate,
    BlockHeader,
    BlockNoteTree,
    BlockNumber,
    Blockchain,
    FeeParameters,
    NullifierTree,
    OutputNoteBatch,
    ProvenBlock,
};
use miden_objects::note::{Note, NoteDetails, NoteType};
use miden_objects::testing::account_id::ACCOUNT_ID_NATIVE_ASSET_FAUCET;
use miden_objects::transaction::{OrderedTransactionHeaders, OutputNote};
use miden_objects::{Felt, FieldElement, MAX_OUTPUT_NOTES_PER_BATCH, NoteError, Word, ZERO};
use rand::Rng;
use vm_processor::crypto::RpoRandomCoin;

use crate::mock_chain::chain::AccountCredentials;
use crate::utils::{create_p2any_note, create_spawn_note};
use crate::{AccountState, Auth, MockChain};

/// A builder for a [`MockChain`].
#[derive(Debug, Clone)]
pub struct MockChainBuilder {
    accounts: BTreeMap<AccountId, Account>,
    account_credentials: BTreeMap<AccountId, AccountCredentials>,
    notes: Vec<OutputNote>,
    rng: RpoRandomCoin,
    // Fee parameters.
    native_asset_id: AccountId,
    verification_base_fee: u32,
}

impl MockChainBuilder {
    // CONSTRUCTORS
    // ----------------------------------------------------------------------------------------

    /// Initializes a new mock chain builder with an empty state.
    ///
    /// By default, the `native_asset_id` is set to [`ACCOUNT_ID_NATIVE_ASSET_FAUCET`] and can be
    /// overwritten using [`Self::native_asset_id`].
    ///
    /// The `verification_base_fee` is initialized to 0 which means no fees are required by default.
    pub fn new() -> Self {
        let native_asset_id =
            ACCOUNT_ID_NATIVE_ASSET_FAUCET.try_into().expect("account ID should be valid");

        Self {
            accounts: BTreeMap::new(),
            account_credentials: BTreeMap::new(),
            notes: Vec::new(),
            rng: RpoRandomCoin::new(Default::default()),
            native_asset_id,
            verification_base_fee: 0,
        }
    }

    /// Initializes a new mock chain builder with the provided accounts.
    ///
    /// This method only adds the accounts and cannot not register any seed or authenticator for it.
    /// Calling [`MockChain::build_tx_context`] on accounts added in this way will not work if the
    /// account is new or if they need an authenticator.
    ///
    /// Due to these limitations, prefer using other methods to add accounts to the chain, e.g.
    /// [`MockChainBuilder::add_account_from_builder`].
    pub fn with_accounts(accounts: impl IntoIterator<Item = Account>) -> anyhow::Result<Self> {
        let mut builder = Self::new();

        for account in accounts {
            builder.add_account(account)?;
        }

        Ok(builder)
    }

    // BUILDER METHODS
    // ----------------------------------------------------------------------------------------

    /// Sets the native asset ID of the chain.
    ///
    /// This must be a fungible faucet [`AccountId`] and is the asset in which fees will be accepted
    /// by the transaction kernel.
    pub fn native_asset_id(mut self, native_asset_id: AccountId) -> Self {
        self.native_asset_id = native_asset_id;
        self
    }

    /// Sets the `verification_base_fee` of the chain.
    ///
    /// See [`FeeParameters`] for more details.
    pub fn verification_base_fee(mut self, verification_base_fee: u32) -> Self {
        self.verification_base_fee = verification_base_fee;
        self
    }

    /// Consumes the builder, creates the genesis block of the chain and returns the [`MockChain`].
    pub fn build(self) -> anyhow::Result<MockChain> {
        // Create the genesis block, consisting of the provided accounts and notes.
        let block_account_updates: Vec<BlockAccountUpdate> = self
            .accounts
            .into_values()
            .map(|account| {
                BlockAccountUpdate::new(
                    account.id(),
                    account.commitment(),
                    AccountUpdateDetails::New(account),
                )
            })
            .collect();

        let account_tree = AccountTree::with_entries(
            block_account_updates
                .iter()
                .map(|account| (account.account_id(), account.final_state_commitment())),
        )
        .context("failed to create genesis account tree")?;

        let note_chunks = self.notes.into_iter().chunks(MAX_OUTPUT_NOTES_PER_BATCH);
        let output_note_batches: Vec<OutputNoteBatch> = note_chunks
            .into_iter()
            .map(|batch_notes| batch_notes.into_iter().enumerate().collect::<Vec<_>>())
            .collect();

        let created_nullifiers = Vec::new();
        let transactions = OrderedTransactionHeaders::new_unchecked(Vec::new());

        let note_tree = BlockNoteTree::from_note_batches(&output_note_batches)
            .context("failed to create block note tree")?;

        let version = 0;
        let prev_block_commitment = Word::empty();
        let block_num = BlockNumber::from(0u32);
        let chain_commitment = Blockchain::new().commitment();
        let account_root = account_tree.root();
        let nullifier_root = NullifierTree::new().root();
        let note_root = note_tree.root();
        let tx_commitment = transactions.commitment();
        let tx_kernel_commitment = TransactionKernel::kernel_commitment();
        let proof_commitment = Word::empty();
        let timestamp = MockChain::TIMESTAMP_START_SECS;
        let fee_parameters = FeeParameters::new(self.native_asset_id, self.verification_base_fee)
            .context("failed to construct fee parameters")?;

        let header = BlockHeader::new(
            version,
            prev_block_commitment,
            block_num,
            chain_commitment,
            account_root,
            nullifier_root,
            note_root,
            tx_commitment,
            tx_kernel_commitment,
            proof_commitment,
            fee_parameters,
            timestamp,
        );

        let genesis_block = ProvenBlock::new_unchecked(
            header,
            block_account_updates,
            output_note_batches,
            created_nullifiers,
            transactions,
        );

        MockChain::from_genesis_block(genesis_block, account_tree, self.account_credentials)
    }

    // ACCOUNT METHODS
    // ----------------------------------------------------------------------------------------

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
                .set_item(
                    memory::FAUCET_STORAGE_DATA_SLOT,
                    Word::from([ZERO, ZERO, ZERO, Felt::new(issuance)]),
                )
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
                AccountMockComponent::new_with_empty_slots()
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
                AccountMockComponent::new_with_slots(slots.into_iter().collect())
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
    /// This method only adds the account and does not store its account credentials (seed and
    /// authenticator) for it. Calling [`MockChain::build_tx_context`] on accounts added in this
    /// way will not work if the account is new or if they need an authenticator.
    ///
    /// Due to these limitations, prefer using other methods to add accounts to the chain, e.g.
    /// [`MockChainBuilder::add_account_from_builder`].
    pub fn add_account(&mut self, account: Account) -> anyhow::Result<()> {
        self.accounts.insert(account.id(), account);

        // This returns a Result to be conservative in case we need to return an error in the future
        // and do not want to break this API.
        Ok(())
    }

    // NOTE METHODS
    // ----------------------------------------------------------------------------------------

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
    pub fn add_p2ide_note(
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

    /// Adds a public SWAP [`OutputNote`] to the list of genesis notes.
    pub fn add_swap_note(
        &mut self,
        sender: AccountId,
        offered_asset: Asset,
        requested_asset: Asset,
        payback_note_type: NoteType,
    ) -> anyhow::Result<(Note, NoteDetails)> {
        let (swap_note, payback_note) = create_swap_note(
            sender,
            offered_asset,
            requested_asset,
            NoteType::Public,
            Felt::ZERO,
            payback_note_type,
            Felt::ZERO,
            &mut self.rng,
        )?;

        self.add_note(OutputNote::Full(swap_note.clone()));

        Ok((swap_note, payback_note))
    }

    /// Adds a public `SPAWN` note to the list of genesis notes.
    ///
    /// A `SPAWN` note contains a note script that creates all `output_notes` that get passed as a
    /// parameter.
    pub fn add_spawn_note<'note>(
        &mut self,
        sender_id: AccountId,
        output_notes: impl IntoIterator<Item = &'note Note>,
    ) -> anyhow::Result<Note> {
        let output_notes = output_notes.into_iter().collect();
        let note = create_spawn_note(sender_id, output_notes)?;

        self.add_note(OutputNote::Full(note.clone()));

        Ok(note)
    }

    /// Creates a new P2ID note with the provided amount of the native fee asset of the chain.
    ///
    /// The native asset ID of the asset can be set using [`Self::native_asset_id`]. By default it
    /// is [`ACCOUNT_ID_NATIVE_ASSET_FAUCET`].
    ///
    /// In the created [`MockChain`], the note will be immediately spendable by `target_account_id`.
    pub fn add_p2id_note_with_fee(
        &mut self,
        target_account_id: AccountId,
        amount: u64,
    ) -> anyhow::Result<Note> {
        let fee_asset = self.native_fee_asset(amount)?;
        let note = self.add_p2id_note(
            self.native_asset_id,
            target_account_id,
            &[Asset::from(fee_asset)],
            NoteType::Public,
        )?;

        Ok(note)
    }

    // HELPER FUNCTIONS
    // ----------------------------------------------------------------------------------------

    /// Constructs a fungible asset based on the native asset ID and the provided amount.
    fn native_fee_asset(&self, amount: u64) -> anyhow::Result<FungibleAsset> {
        FungibleAsset::new(self.native_asset_id, amount).context("failed to create fee asset")
    }
}

impl Default for MockChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}
