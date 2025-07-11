use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::ToString,
    vec::Vec,
};

use anyhow::Context;
use miden_block_prover::{LocalBlockProver, ProvenBlockError};
use miden_lib::{
    account::{faucets::BasicFungibleFaucet, wallets::BasicWallet},
    note::{create_p2id_note, create_p2ide_note},
    transaction::{TransactionKernel, memory},
};
use miden_objects::{
    MAX_BATCHES_PER_BLOCK, MAX_OUTPUT_NOTES_PER_BATCH, NoteError,
    account::{
        Account, AccountBuilder, AccountId, AccountStorageMode, AccountType, StorageSlot,
        delta::AccountUpdateDetails,
    },
    asset::{Asset, TokenSymbol},
    batch::{ProposedBatch, ProvenBatch},
    block::{
        AccountTree, AccountWitness, BlockAccountUpdate, BlockHeader, BlockInputs, BlockNoteTree,
        BlockNumber, Blockchain, NullifierTree, NullifierWitness, ProposedBlock, ProvenBlock,
    },
    crypto::merkle::SmtProof,
    note::{Note, NoteHeader, NoteId, NoteInclusionProof, NoteType, Nullifier},
    transaction::{
        AccountInputs, ExecutedTransaction, InputNote, InputNotes, OrderedTransactionHeaders,
        OutputNote, PartialBlockchain, ProvenTransaction, TransactionHeader, TransactionInputs,
    },
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use vm_processor::{Felt, Word, ZERO, crypto::RpoRandomCoin};

use super::note::MockChainNote;
use crate::{
    Auth, MockFungibleFaucet, ProvenTransactionExt, TransactionContextBuilder,
    mock_chain::account::MockAccount,
};

// MOCK CHAIN
// ================================================================================================

/// The [`MockChain`] simulates a simplified blockchain environment for testing purposes.
/// It allows creating and managing accounts, minting assets, executing transactions, and applying
/// state updates.
///
/// This struct is designed to mock transaction workflows, asset transfers, and
/// note creation in a test setting. Once entities are set up, [`TransactionContextBuilder`] objects
/// can be obtained in order to execute transactions accordingly.
///
/// On a high-level, there are two ways to interact with the mock chain:
/// - Generating transactions yourself and adding them to the mock chain "mempool" using
///   [`MockChain::add_pending_executed_transaction`] or
///   [`MockChain::add_pending_proven_transaction`]. Once some transactions have been added, they
///   can be proven into a block using [`MockChain::prove_next_block`], which commits them to the
///   chain state.
/// - Using any of the other pending APIs to _magically_ add new notes, accounts or nullifiers in
///   the next block. For example, [`MockChain::add_pending_p2id_note`] will create a new P2ID note
///   in the next proven block, without actually containing a transaction that creates that note.
///
/// Both approaches can be mixed in the same block, within limits. In particular, avoid modification
/// of the _same_ entities using both regular transactions and the magic pending APIs.
///
/// The mock chain uses the batch and block provers underneath to process pending transactions, so
/// the generated blocks are realistic and indistinguishable from a real node. The only caveat is
/// that no real ZK proofs are generated or validated as part of transaction, batch or block
/// building. If realistic data is important for your use case, avoid using any pending APIs except
/// for [`MockChain::add_pending_executed_transaction`] and
/// [`MockChain::add_pending_proven_transaction`].
///
/// # Examples
///
/// ## Create mock objects and build a transaction context
/// ```
/// # use anyhow::Result;
/// # use miden_objects::{Felt, asset::FungibleAsset, note::NoteType};
/// # use miden_testing::{Auth, MockChain, TransactionContextBuilder};
///
/// # fn main() -> Result<()> {
/// let mut mock_chain = MockChain::new();
///
/// let faucet = mock_chain.add_pending_new_faucet(Auth::BasicAuth, "USDT", 100_000)?;
/// let _asset = faucet.mint(1000);
///
/// let sender = mock_chain.add_pending_new_wallet(Auth::BasicAuth);
/// let target = mock_chain.add_pending_new_wallet(Auth::BasicAuth);
///
/// let note = mock_chain.add_pending_p2id_note(
///     faucet.id(),
///     target.id(),
///     &[FungibleAsset::mock(10)],
///     NoteType::Public,
/// )?;
///
/// mock_chain.prove_next_block()?;
///
/// let tx_context = mock_chain.build_tx_context(sender.id(), &[note.id()], &[])?.build()?;
/// let result = tx_context.execute();
/// # Ok(())
/// # }
/// ```
///
/// ## Executing a simple transaction
/// ```
/// # use anyhow::Result;
/// # use miden_objects::{
/// #    asset::{Asset, FungibleAsset},
/// #    note::NoteType,
/// # };
/// # use miden_testing::{Auth, MockChain};
///
/// # fn main() -> Result<()> {
/// let mut mock_chain = MockChain::new();
///
/// // Add a recipient wallet.
/// let receiver = mock_chain.add_pending_new_wallet(Auth::BasicAuth);
///
/// // Add a wallet with assets.
/// let sender = mock_chain.add_pending_existing_wallet(Auth::IncrNonce, vec![]);
/// let fungible_asset = FungibleAsset::mock(10).unwrap_fungible();
///
/// // Add a pending P2ID note to the chain.
/// let note = mock_chain.add_pending_p2id_note(
///     sender.id(),
///     receiver.id(),
///     &[Asset::Fungible(fungible_asset)],
///     NoteType::Public,
/// )?;
///
/// mock_chain.prove_next_block()?;
///
/// let transaction = mock_chain
///     .build_tx_context(receiver.id(), &[note.id()], &[])?
///     .build()?
///     .execute()?;
///
/// // Add the transaction to the mock chain's "mempool" of pending transactions.
/// mock_chain.add_pending_executed_transaction(&transaction);
///
/// // Prove the next block to include the transaction in the chain state.
/// mock_chain.prove_next_block()?;
///
/// assert_eq!(
///     mock_chain
///         .committed_account(receiver.id())?
///         .vault()
///         .get_balance(fungible_asset.faucet_id())?,
///     fungible_asset.amount()
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MockChain {
    /// An append-only structure used to represent the history of blocks produced for this chain.
    chain: Blockchain,

    /// History of produced blocks.
    blocks: Vec<ProvenBlock>,

    /// Tree containing all nullifiers.
    nullifier_tree: NullifierTree,

    /// Tree containing the state commitments of all accounts.
    account_tree: AccountTree,

    /// Objects that have not yet been finalized.
    ///
    /// These will become available once the block is proven.
    ///
    /// Note:
    /// - The [Note]s in this container do not have the `proof` set.
    pending_objects: PendingObjects,

    /// Transactions that have been submitted to the chain but have not yet been included in a
    /// block.
    pending_transactions: Vec<ProvenTransaction>,

    /// NoteID |-> MockChainNote mapping to simplify note retrieval.
    committed_notes: BTreeMap<NoteId, MockChainNote>,

    /// AccountId |-> MockAccount mapping to simplify transaction creation. Latest known account
    /// state is maintained for each account here.
    ///
    /// The map always holds the most recent *public* state known for every account. For private
    /// accounts, however, transactions do not emit the post-transaction state, so their entries
    /// remain at the last observed state.
    committed_accounts: BTreeMap<AccountId, MockAccount>,

    // The RNG used to generate note serial numbers, account seeds or cryptographic keys.
    rng: ChaCha20Rng,
}

impl MockChain {
    // CONSTANTS
    // ----------------------------------------------------------------------------------------

    /// The timestamp of the genesis block of the chain. Chosen as an easily readable number.
    pub const TIMESTAMP_START_SECS: u32 = 1700000000;

    /// The number of seconds by which a block's timestamp increases over the previous block's
    /// timestamp, unless overwritten when calling [`Self::prove_next_block_at`].
    pub const TIMESTAMP_STEP_SECS: u32 = 10;

    // CONSTRUCTORS
    // ----------------------------------------------------------------------------------------

    /// Creates a new `MockChain` with an empty genesis block.
    pub fn new() -> Self {
        Self::with_accounts(&[]).expect("empty mockchain is valid")
    }

    /// Creates a new `MockChain` with a genesis block containing the provided accounts.
    pub fn with_accounts(accounts: &[Account]) -> anyhow::Result<Self> {
        let (genesis_block, account_tree) = create_genesis_state(accounts.iter().cloned())
            .context("failed to build account from builder")?;

        let mut chain = MockChain {
            chain: Blockchain::default(),
            blocks: vec![],
            nullifier_tree: NullifierTree::default(),
            account_tree,
            pending_objects: PendingObjects::new(),
            pending_transactions: Vec::new(),
            committed_notes: BTreeMap::new(),
            committed_accounts: BTreeMap::new(),
            // Initialize RNG with default seed.
            rng: ChaCha20Rng::from_seed(Default::default()),
        };

        // We do not have to apply the tree changes, because the account tree is already initialized
        // and the nullifier tree is empty at genesis.
        chain
            .apply_block(genesis_block)
            .context("failed to build account from builder")?;

        debug_assert_eq!(chain.blocks.len(), 1);
        debug_assert_eq!(chain.account_tree.num_accounts(), accounts.len());
        debug_assert_eq!(chain.committed_accounts.len(), accounts.len());
        for added_account in accounts {
            debug_assert_eq!(
                chain.account_tree.get(added_account.id()),
                added_account.commitment()
            );
            debug_assert_eq!(
                chain.committed_account(added_account.id())?.commitment(),
                added_account.commitment(),
            );
        }

        Ok(chain)
    }

    // PUBLIC ACCESSORS
    // ----------------------------------------------------------------------------------------

    /// Returns a reference to the current [`Blockchain`].
    pub fn blockchain(&self) -> &Blockchain {
        &self.chain
    }

    /// Returns a [`PartialBlockchain`] instantiated from the current [`Blockchain`] and with
    /// authentication paths for all all blocks in the chain.
    pub fn latest_partial_blockchain(&self) -> PartialBlockchain {
        // We have to exclude the latest block because we need to fetch the state of the chain at
        // that latest block, which does not include itself.
        let block_headers =
            self.blocks.iter().map(|b| b.header()).take(self.blocks.len() - 1).cloned();

        PartialBlockchain::from_blockchain(&self.chain, block_headers)
            .expect("blockchain should be valid by construction")
    }

    /// Creates a new [`PartialBlockchain`] with all reference blocks in the given iterator except
    /// for the latest block header in the chain and returns that latest block header.
    ///
    /// The intended use for the latest block header is to become the reference block of a new
    /// transaction batch or block.
    pub fn latest_selective_partial_blockchain(
        &self,
        reference_blocks: impl IntoIterator<Item = BlockNumber>,
    ) -> anyhow::Result<(BlockHeader, PartialBlockchain)> {
        let latest_block_header = self.latest_block_header();
        // Deduplicate block numbers so each header will be included just once. This is required so
        // PartialBlockchain::from_blockchain does not panic.
        let reference_blocks: BTreeSet<_> = reference_blocks.into_iter().collect();

        // Include all block headers of the reference blocks except the latest block.
        let mut block_headers = Vec::new();

        for block_ref_num in &reference_blocks {
            let block_index = block_ref_num.as_usize();
            let block = self
                .blocks
                .get(block_index)
                .ok_or_else(|| anyhow::anyhow!("block {} not found in chain", block_ref_num))?;
            let block_header = block.header().clone();
            // Exclude the latest block header
            if block_header.commitment() != latest_block_header.commitment() {
                block_headers.push(block_header);
            }
        }

        let partial_blockchain = PartialBlockchain::from_blockchain(&self.chain, block_headers)?;

        Ok((latest_block_header, partial_blockchain))
    }

    /// Returns a map of [`AccountWitness`]es for the requested account IDs from the current
    /// [`AccountTree`] in the chain.
    pub fn account_witnesses(
        &self,
        account_ids: impl IntoIterator<Item = AccountId>,
    ) -> BTreeMap<AccountId, AccountWitness> {
        let mut account_witnesses = BTreeMap::new();

        for account_id in account_ids {
            let witness = self.account_tree.open(account_id);
            account_witnesses.insert(account_id, witness);
        }

        account_witnesses
    }

    /// Returns a map of [`NullifierWitness`]es for the requested nullifiers from the current
    /// [`NullifierTree`] in the chain.
    pub fn nullifier_witnesses(
        &self,
        nullifiers: impl IntoIterator<Item = Nullifier>,
    ) -> BTreeMap<Nullifier, NullifierWitness> {
        let mut nullifier_proofs = BTreeMap::new();

        for nullifier in nullifiers {
            let witness = self.nullifier_tree.open(&nullifier);
            nullifier_proofs.insert(nullifier, witness);
        }

        nullifier_proofs
    }

    /// Returns all note inclusion proofs for the requested note IDs, **if they are available for
    /// consumption**. Therefore, not all of the requested notes will be guaranteed to have an entry
    /// in the returned map.
    pub fn unauthenticated_note_proofs(
        &self,
        notes: impl IntoIterator<Item = NoteId>,
    ) -> BTreeMap<NoteId, NoteInclusionProof> {
        let mut proofs = BTreeMap::default();
        for note in notes {
            if let Some(input_note) = self.committed_notes.get(&note) {
                proofs.insert(note, input_note.inclusion_proof().clone());
            }
        }

        proofs
    }

    /// Returns a reference to the latest [`BlockHeader`] in the chain.
    pub fn latest_block_header(&self) -> BlockHeader {
        let chain_tip =
            self.chain.chain_tip().expect("chain should contain at least the genesis block");
        self.blocks[chain_tip.as_usize()].header().clone()
    }

    /// Returns the [`BlockHeader`] with the specified `block_number`.
    ///
    /// # Panics
    ///
    /// - If the block number does not exist in the chain.
    pub fn block_header(&self, block_number: usize) -> BlockHeader {
        self.blocks[block_number].header().clone()
    }

    /// Returns a reference to slice of all created proven blocks.
    pub fn proven_blocks(&self) -> &[ProvenBlock] {
        &self.blocks
    }

    /// Returns a reference to the nullifier tree.
    pub fn nullifier_tree(&self) -> &NullifierTree {
        &self.nullifier_tree
    }

    /// Returns the map of note IDs to committed notes.
    ///
    /// These notes are committed for authenticated consumption.
    pub fn committed_notes(&self) -> &BTreeMap<NoteId, MockChainNote> {
        &self.committed_notes
    }

    /// Returns an [`InputNote`] for the given note ID. If the note does not exist or is not
    /// public, `None` is returned.
    pub fn get_public_note(&self, note_id: &NoteId) -> Option<InputNote> {
        let note = self.committed_notes.get(note_id)?;
        note.clone().try_into().ok()
    }

    /// Returns a reference to the account identified by the given account ID.
    ///
    /// The account is retrieved with the latest state known to the [`MockChain`].
    pub fn committed_account(&self, account_id: AccountId) -> anyhow::Result<&Account> {
        self.committed_accounts
            .get(&account_id)
            .map(|mock_account| mock_account.account())
            .with_context(|| format!("account {account_id} not found in committed accounts"))
    }

    /// Returns a reference to the [`AccountTree`] of the chain.
    pub fn account_tree(&self) -> &AccountTree {
        &self.account_tree
    }

    // BATCH APIS
    // ----------------------------------------------------------------------------------------

    /// Proposes a new transaction batch from the provided transactions and returns it.
    ///
    /// This method does not modify the chain state.
    pub fn propose_transaction_batch<I>(
        &self,
        txs: impl IntoIterator<Item = ProvenTransaction, IntoIter = I>,
    ) -> anyhow::Result<ProposedBatch>
    where
        I: Iterator<Item = ProvenTransaction> + Clone,
    {
        let transactions: Vec<_> = txs.into_iter().map(alloc::sync::Arc::new).collect();

        let (batch_reference_block, partial_blockchain, unauthenticated_note_proofs) = self
            .get_batch_inputs(
                transactions.iter().map(|tx| tx.ref_block_num()),
                transactions
                    .iter()
                    .flat_map(|tx| tx.unauthenticated_notes().map(NoteHeader::id)),
            )?;

        Ok(ProposedBatch::new(
            transactions,
            batch_reference_block,
            partial_blockchain,
            unauthenticated_note_proofs,
        )?)
    }

    /// Mock-proves a proposed transaction batch from the provided [`ProposedBatch`] and returns it.
    ///
    /// This method does not modify the chain state.
    pub fn prove_transaction_batch(
        &self,
        proposed_batch: ProposedBatch,
    ) -> anyhow::Result<ProvenBatch> {
        let (
            transactions,
            block_header,
            _partial_blockchain,
            _unauthenticated_note_proofs,
            id,
            account_updates,
            input_notes,
            output_notes,
            batch_expiration_block_num,
        ) = proposed_batch.into_parts();

        // SAFETY: This satisfies the requirements of the ordered tx headers.
        let tx_headers = OrderedTransactionHeaders::new_unchecked(
            transactions
                .iter()
                .map(AsRef::as_ref)
                .map(TransactionHeader::from)
                .collect::<Vec<_>>(),
        );

        Ok(ProvenBatch::new(
            id,
            block_header.commitment(),
            block_header.block_num(),
            account_updates,
            input_notes,
            output_notes,
            batch_expiration_block_num,
            tx_headers,
        )?)
    }

    // BLOCK APIS
    // ----------------------------------------------------------------------------------------

    /// Proposes a new block from the provided batches with the given timestamp and returns it.
    ///
    /// This method does not modify the chain state.
    pub fn propose_block_at<I>(
        &self,
        batches: impl IntoIterator<Item = ProvenBatch, IntoIter = I>,
        timestamp: u32,
    ) -> anyhow::Result<ProposedBlock>
    where
        I: Iterator<Item = ProvenBatch> + Clone,
    {
        let batches: Vec<_> = batches.into_iter().collect();

        let block_inputs = self
            .get_block_inputs(batches.iter())
            .context("could not retrieve block inputs")?;

        let proposed_block = ProposedBlock::new_at(block_inputs, batches, timestamp)
            .context("failed to create proposed block")?;

        Ok(proposed_block)
    }

    /// Proposes a new block from the provided batches and returns it.
    ///
    /// This method does not modify the chain state.
    pub fn propose_block<I>(
        &self,
        batches: impl IntoIterator<Item = ProvenBatch, IntoIter = I>,
    ) -> anyhow::Result<ProposedBlock>
    where
        I: Iterator<Item = ProvenBatch> + Clone,
    {
        // We can't access system time because we are in a no-std environment, so we use the
        // minimally correct next timestamp.
        let timestamp = self.latest_block_header().timestamp() + 1;

        self.propose_block_at(batches, timestamp)
    }

    /// Mock-proves a proposed block into a proven block and returns it.
    ///
    /// This method does not modify the chain state.
    pub fn prove_block(
        &self,
        proposed_block: ProposedBlock,
    ) -> Result<ProvenBlock, ProvenBlockError> {
        LocalBlockProver::new(0).prove_without_batch_verification(proposed_block)
    }

    // TRANSACTION APIS
    // ----------------------------------------------------------------------------------------

    /// Initializes a [`TransactionContextBuilder`].
    ///
    /// Depending on the provided `input`, the builder is initialized differently:
    /// - [`TxContextInput::AccountId`]: Initialize the builder with [`TransactionInputs`] fetched
    ///   from the chain for the public account identified by the ID.
    /// - [`TxContextInput::Account`]: Initialize the builder with [`TransactionInputs`] where the
    ///   account is passed as-is to the inputs.
    /// - [`TxContextInput::ExecutedTransaction`]: Initialize the builder with [`TransactionInputs`]
    ///   where the account passed to the inputs is the final account of the executed transaction.
    ///   This is the initial account of the transaction with the account delta applied.
    ///
    /// In all cases, if the chain contains a seed or authenticator for the account, they are added
    /// to the builder.
    ///
    /// [`TxContextInput::Account`] and [`TxContextInput::ExecutedTransaction`] can be used to build
    /// a chain of transactions against the same account that build on top of each other. For
    /// example, transaction A modifies an account from state 0 to 1, and transaction B modifies
    /// it from state 1 to 2.
    pub fn build_tx_context(
        &self,
        input: impl Into<TxContextInput>,
        note_ids: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionContextBuilder> {
        let mock_account = match input.into() {
            TxContextInput::AccountId(account_id) => {
                if account_id.is_private() {
                    return Err(anyhow::anyhow!(
                        "transaction contexts for private accounts should be created with TxContextInput::Account"
                    ));
                }

                self.committed_accounts
                    .get(&account_id)
                    .with_context(|| {
                        format!("account {account_id} not found in committed accounts")
                    })?
                    .clone()
            },
            TxContextInput::Account(account) => {
                let committed_account = self.committed_accounts.get(&account.id());
                let authenticator = committed_account.and_then(|a| a.authenticator());
                let seed = committed_account.and_then(|a| a.seed());
                MockAccount::new(account, seed.cloned(), authenticator.cloned())
            },
            TxContextInput::ExecutedTransaction(executed_transaction) => {
                let mut initial_account = executed_transaction.initial_account().clone();
                initial_account
                    .apply_delta(executed_transaction.account_delta())
                    .context("could not apply delta from previous transaction")?;

                let committed_account = self.committed_accounts.get(&initial_account.id());
                let authenticator = committed_account.and_then(|a| a.authenticator());
                let seed = committed_account.and_then(|a| a.seed());
                MockAccount::new(initial_account, seed.cloned(), authenticator.cloned())
            },
        };

        let tx_inputs = self
            .get_transaction_inputs(
                mock_account.account().clone(),
                mock_account.seed().cloned(),
                note_ids,
                unauthenticated_notes,
            )
            .context("failed to gather transaction inputs")?;

        let tx_context_builder = TransactionContextBuilder::new(mock_account.account().clone())
            .authenticator(mock_account.authenticator().cloned())
            .account_seed(mock_account.seed().cloned())
            .tx_inputs(tx_inputs);

        Ok(tx_context_builder)
    }

    // INPUTS APIS
    // ----------------------------------------------------------------------------------------

    /// Returns a valid [`TransactionInputs`] for the specified entities.
    pub fn get_transaction_inputs(
        &self,
        account: Account,
        account_seed: Option<Word>,
        notes: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionInputs> {
        let block = self.blocks.last().expect("at least one block should have been created");

        let mut input_notes = vec![];
        let mut block_headers_map: BTreeMap<BlockNumber, BlockHeader> = BTreeMap::new();
        for note in notes {
            let input_note: InputNote = self
                .committed_notes
                .get(note)
                .with_context(|| format!("note with id {note} not found"))?
                .clone()
                .try_into()
                .context("note error")?;

            let note_block_num = input_note
                .location()
                .with_context(|| format!("note location not available: {note}"))?
                .block_num();

            if note_block_num != block.header().block_num() {
                let block_header = self
                    .blocks
                    .get(note_block_num.as_usize())
                    .with_context(|| format!("block {note_block_num} not found in chain"))?
                    .header()
                    .clone();
                block_headers_map.insert(note_block_num, block_header);
            }

            input_notes.push(input_note);
        }

        for note in unauthenticated_notes {
            input_notes.push(InputNote::Unauthenticated { note: note.clone() })
        }

        let block_headers = block_headers_map.values().cloned();
        let mmr = PartialBlockchain::from_blockchain(&self.chain, block_headers)?;

        let input_notes = InputNotes::new(input_notes)?;

        Ok(TransactionInputs::new(
            account,
            account_seed,
            block.header().clone(),
            mmr,
            input_notes,
        )?)
    }

    /// Returns inputs for a transaction batch for all the reference blocks of the provided
    /// transactions.
    pub fn get_batch_inputs(
        &self,
        tx_reference_blocks: impl IntoIterator<Item = BlockNumber>,
        unauthenticated_notes: impl Iterator<Item = NoteId>,
    ) -> anyhow::Result<(BlockHeader, PartialBlockchain, BTreeMap<NoteId, NoteInclusionProof>)>
    {
        // Fetch note proofs for notes that exist in the chain.
        let unauthenticated_note_proofs = self.unauthenticated_note_proofs(unauthenticated_notes);

        // We also need to fetch block inclusion proofs for any of the blocks that contain
        // unauthenticated notes for which we want to prove inclusion.
        let required_blocks = tx_reference_blocks.into_iter().chain(
            unauthenticated_note_proofs
                .values()
                .map(|note_proof| note_proof.location().block_num()),
        );

        let (batch_reference_block, partial_block_chain) =
            self.latest_selective_partial_blockchain(required_blocks)?;

        Ok((batch_reference_block, partial_block_chain, unauthenticated_note_proofs))
    }

    /// Gets foreign account inputs to execute FPI transactions.
    pub fn get_foreign_account_inputs(
        &self,
        account_id: AccountId,
    ) -> anyhow::Result<AccountInputs> {
        let account = self.committed_account(account_id)?;

        let account_witness = self.account_tree().open(account_id);
        assert_eq!(account_witness.state_commitment(), account.commitment());

        let mut storage_map_proofs = vec![];
        for slot in account.storage().slots() {
            // if there are storage maps, we populate the merkle store and advice map
            if let StorageSlot::Map(map) = slot {
                let proofs: Vec<SmtProof> = map.entries().map(|(key, _)| map.open(key)).collect();
                storage_map_proofs.extend(proofs);
            }
        }

        Ok(AccountInputs::new(account.into(), account_witness))
    }

    /// Gets the inputs for a block for the provided batches.
    pub fn get_block_inputs<'batch, I>(
        &self,
        batch_iter: impl IntoIterator<Item = &'batch ProvenBatch, IntoIter = I>,
    ) -> anyhow::Result<BlockInputs>
    where
        I: Iterator<Item = &'batch ProvenBatch> + Clone,
    {
        let batch_iterator = batch_iter.into_iter();

        let unauthenticated_note_proofs =
            self.unauthenticated_note_proofs(batch_iterator.clone().flat_map(|batch| {
                batch.input_notes().iter().filter_map(|note| note.header().map(NoteHeader::id))
            }));

        let (block_reference_block, partial_blockchain) = self
            .latest_selective_partial_blockchain(
                batch_iterator.clone().map(ProvenBatch::reference_block_num).chain(
                    unauthenticated_note_proofs.values().map(|proof| proof.location().block_num()),
                ),
            )?;

        let account_witnesses =
            self.account_witnesses(batch_iterator.clone().flat_map(ProvenBatch::updated_accounts));

        let nullifier_proofs =
            self.nullifier_witnesses(batch_iterator.flat_map(ProvenBatch::created_nullifiers));

        Ok(BlockInputs::new(
            block_reference_block,
            partial_blockchain,
            account_witnesses,
            nullifier_proofs,
            unauthenticated_note_proofs,
        ))
    }

    // PUBLIC MUTATORS
    // ----------------------------------------------------------------------------------------

    /// Creates the next block in the mock chain.
    ///
    /// This will make all the objects currently pending available for use.
    pub fn prove_next_block(&mut self) -> anyhow::Result<ProvenBlock> {
        self.prove_block_inner(None)
    }

    /// Proves the next block in the mock chain at the given timestamp.
    pub fn prove_next_block_at(&mut self, timestamp: u32) -> anyhow::Result<ProvenBlock> {
        self.prove_block_inner(Some(timestamp))
    }

    /// Proves new blocks until the block with the given target block number has been created.
    ///
    /// For example, if the latest block is `5` and this function is called with `10`, then blocks
    /// `6..=10` will be created and block 10 will be returned.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the given block number is smaller or equal to the number of the latest block in the chain.
    pub fn prove_until_block(
        &mut self,
        target_block_num: impl Into<BlockNumber>,
    ) -> anyhow::Result<ProvenBlock> {
        let target_block_num = target_block_num.into();
        let latest_block_num = self.latest_block_header().block_num();
        assert!(
            target_block_num > latest_block_num,
            "target block number must be greater than the number of the latest block in the chain"
        );

        let mut last_block = None;
        for _ in latest_block_num.as_usize()..target_block_num.as_usize() {
            last_block = Some(self.prove_next_block()?);
        }

        Ok(last_block.expect("at least one block should have been created"))
    }

    /// Sets the seed for the internal RNG.
    pub fn set_rng_seed(&mut self, seed: [u8; 32]) {
        self.rng = ChaCha20Rng::from_seed(seed);
    }

    // PUBLIC MUTATORS (PENDING APIS)
    // ----------------------------------------------------------------------------------------

    /// Adds the given [`ExecutedTransaction`] to the list of pending transactions.
    ///
    /// A block has to be created to apply the transaction effects to the chain state, e.g. using
    /// [`MockChain::prove_next_block`].
    ///
    /// Returns the resulting state of the executing account after executing the transaction.
    pub fn add_pending_executed_transaction(
        &mut self,
        transaction: &ExecutedTransaction,
    ) -> anyhow::Result<Account> {
        let mut account = transaction.initial_account().clone();
        account.apply_delta(transaction.account_delta())?;

        // This essentially transforms an executed tx into a proven tx with a dummy proof.
        let proven_tx = ProvenTransaction::from_executed_transaction_mocked(transaction.clone());

        self.pending_transactions.push(proven_tx);

        Ok(account)
    }

    /// Adds the given [`ProvenTransaction`] to the list of pending transactions.
    ///
    /// A block has to be created to apply the transaction effects to the chain state, e.g. using
    /// [`MockChain::prove_next_block`].
    pub fn add_pending_proven_transaction(&mut self, transaction: ProvenTransaction) {
        self.pending_transactions.push(transaction);
    }

    /// Adds the given [`OutputNote`] to the list of pending notes.
    ///
    /// A block has to be created to add the note to that block and make it available in the chain
    /// state, e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_note(&mut self, note: OutputNote) {
        self.pending_objects.output_notes.push(note);
    }

    /// Adds a plain P2ID [`OutputNote`] to the list of pending notes.
    ///
    /// The note is immediately spendable by `target_account_id` and carries no
    /// additional reclaim or timelock conditions.
    pub fn add_pending_p2id_note(
        &mut self,
        sender_account_id: AccountId,
        target_account_id: AccountId,
        asset: &[Asset],
        note_type: NoteType,
    ) -> Result<Note, NoteError> {
        let mut rng = RpoRandomCoin::new(Word::empty());

        let note = create_p2id_note(
            sender_account_id,
            target_account_id,
            asset.to_vec(),
            note_type,
            Default::default(),
            &mut rng,
        )?;

        self.add_pending_note(OutputNote::Full(note.clone()));
        Ok(note)
    }

    /// Adds a P2IDE [`OutputNote`] (pay‑to‑ID‑escape) to the list of pending notes.
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
        let mut rng = RpoRandomCoin::new(Word::empty());

        let note = create_p2ide_note(
            sender_account_id,
            target_account_id,
            asset.to_vec(),
            reclaim_height,
            timelock_height,
            note_type,
            Default::default(),
            &mut rng,
        )?;

        self.add_pending_note(OutputNote::Full(note.clone()));
        Ok(note)
    }

    /// Adds the [`Nullifier`] to the list of pending nullifiers.
    ///
    /// A block has to be created to add the nullifier to the nullifier tree as part of that block,
    /// e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_nullifier(&mut self, nullifier: Nullifier) {
        self.pending_objects.created_nullifiers.push(nullifier);
    }

    /// Adds a new public [`BasicWallet`] account to the list of pending accounts.
    ///
    /// A block has to be created to add the account to the chain state as part of that block,
    /// e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_new_wallet(&mut self, auth_method: Auth) -> Account {
        let account_builder = AccountBuilder::new(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(BasicWallet);

        self.add_pending_account_from_builder(auth_method, account_builder, AccountState::New)
            .expect("failed to add pending account from builder")
    }

    /// Adds an existing public [`BasicWallet`] account with nonce `1` to the list of pending
    /// accounts.
    ///
    /// A block has to be created to add the account to the chain state as part of that block,
    /// e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_existing_wallet(
        &mut self,
        auth_method: Auth,
        assets: Vec<Asset>,
    ) -> Account {
        let account_builder = Account::builder(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(BasicWallet)
            .with_assets(assets);

        self.add_pending_account_from_builder(auth_method, account_builder, AccountState::Exists)
            .expect("failed to add pending account from builder")
    }

    /// Adds a new public [`BasicFungibleFaucet`] account with the specified authentication method
    /// and the given token metadata to the list of pending accounts.
    ///
    /// A block has to be created to add the account to the chain state as part of that block,
    /// e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_new_faucet(
        &mut self,
        auth_method: Auth,
        token_symbol: &str,
        max_supply: u64,
    ) -> anyhow::Result<MockFungibleFaucet> {
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

        let account =
            self.add_pending_account_from_builder(auth_method, account_builder, AccountState::New)?;

        Ok(MockFungibleFaucet::new(account))
    }

    /// Adds an existing [`BasicFungibleFaucet`] account with the specified authentication method
    /// and the given token metadata to the list of pending accounts.
    ///
    /// A block has to be created to add the account to the chain state as part of that block,
    /// e.g. using [`MockChain::prove_next_block`].
    pub fn add_pending_existing_faucet(
        &mut self,
        auth_method: Auth,
        token_symbol: &str,
        max_supply: u64,
        total_issuance: Option<u64>,
    ) -> anyhow::Result<MockFungibleFaucet> {
        let token_symbol = TokenSymbol::new(token_symbol).context("invalid argument")?;
        let basic_faucet = BasicFungibleFaucet::new(token_symbol, 10u8, Felt::new(max_supply))
            .context("invalid argument")?;

        let mut account_builder = AccountBuilder::new(self.rng.random())
            .storage_mode(AccountStorageMode::Public)
            .with_component(basic_faucet)
            .account_type(AccountType::FungibleFaucet);

        let (auth_component, authenticator) = auth_method.build_component();
        account_builder = account_builder.with_auth_component(auth_component);
        let mut account = account_builder
            .build_existing()
            .context("failed to build account from builder")?;

        // The faucet's reserved slot is initialized to an empty word by default.
        // If total_issuance is set, overwrite it.
        if let Some(issuance) = total_issuance {
            account
                .storage_mut()
                .set_item(
                    memory::FAUCET_STORAGE_DATA_SLOT,
                    Word::new([ZERO, ZERO, ZERO, Felt::new(issuance)]),
                )
                .context("failed to set faucet storage")?;
        }

        // We have to insert these into the committed accounts so the authenticator is available.
        // Without this, the account couldn't be authenticated.
        self.committed_accounts
            .insert(account.id(), MockAccount::new(account.clone(), None, authenticator));
        self.add_pending_account(account.clone());

        Ok(MockFungibleFaucet::new(account))
    }

    /// Adds the [`AccountComponent`](miden_objects::account::AccountComponent) corresponding to
    /// `auth_method` to the account in the builder and builds a new or existing account
    /// depending on `account_state`.
    ///
    /// The account is added to the list of committed accounts _and_, if [`AccountState::Exists`] is
    /// passed, is also added to the list of pending accounts. Adding it to committed accounts
    /// makes the account seed and authenticator available for account creation and
    /// authentication, respectively. If the account exists, then the next block that is created
    /// will add the pending accounts to the chain state.
    pub fn add_pending_account_from_builder(
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

        // Add account to the committed accounts so transaction inputs can be retrieved via the mock
        // chain APIs.
        // We also have to insert these into the committed accounts so the account seed and
        // authenticator are available. Without this, the account couldn't be created or
        // authenticated.
        self.committed_accounts
            .insert(account.id(), MockAccount::new(account.clone(), seed, authenticator));

        // Do not add new accounts to the pending accounts. Usually, new accounts are added in tests
        // to create transactions that create these new accounts in the chain. If we add them to the
        // pending accounts and the test calls prove_next_block (which typically happens to add all
        // pending objects to the chain state as part of the test setup), the account will be added
        // to the chain which means the account-creating transaction fails because the
        // account already exists. So new accounts are added only to the committed accounts
        // so the transaction context APIs work as expected.
        if let AccountState::Exists = account_state {
            self.add_pending_account(account.clone());
        }

        Ok(account)
    }

    /// Adds a new `Account` to the list of pending objects.
    ///
    /// A block has to be created to finalize the new entity.
    pub fn add_pending_account(&mut self, account: Account) {
        let account_id = account.id();
        let account_commitment = account.commitment();
        let update_details = match account.is_private() {
            true => AccountUpdateDetails::Private,
            false => AccountUpdateDetails::New(account),
        };

        self.pending_objects.updated_accounts.insert(
            account_id,
            BlockAccountUpdate::new(account_id, account_commitment, update_details),
        );
    }

    // PRIVATE HELPERS
    // ----------------------------------------------------------------------------------------

    /// Inserts the given block's account updates and created nullifiers into the account tree and
    /// nullifier tree, respectively.
    fn apply_block_tree_updates(&mut self, proven_block: &ProvenBlock) -> anyhow::Result<()> {
        for account_update in proven_block.updated_accounts() {
            self.account_tree
                .insert(account_update.account_id(), account_update.final_state_commitment())
                .context("failed to insert account update into account tree")?;
        }

        for nullifier in proven_block.created_nullifiers() {
            self.nullifier_tree
                .mark_spent(*nullifier, proven_block.header().block_num())
                .context("failed to mark block nullifier as spent")?;

            // TODO: Remove from self.committed_notes. This is not critical to have for now. It is
            // not straightforward, because committed_notes are indexed by note IDs rather than
            // nullifiers, so we'll have to create a second index to do this.
        }

        Ok(())
    }

    /// Applies the given block to the chain state, which means:
    ///
    /// - Updated accounts from the block are updated in the committed accounts.
    /// - Created notes are inserted into the committed notes.
    /// - Consumed notes are removed from the committed notes.
    /// - The block is appended to the [`BlockChain`] and the list of proven blocks.
    fn apply_block(&mut self, proven_block: ProvenBlock) -> anyhow::Result<()> {
        for account_update in proven_block.updated_accounts() {
            match account_update.details() {
                AccountUpdateDetails::New(account) => {
                    let committed_account =
                        self.committed_accounts.get(&account_update.account_id());
                    let authenticator =
                        committed_account.and_then(|account| account.authenticator());
                    let seed = committed_account.and_then(|account| account.seed());

                    self.committed_accounts.insert(
                        account.id(),
                        MockAccount::new(account.clone(), seed.cloned(), authenticator.cloned()),
                    );
                },
                AccountUpdateDetails::Delta(account_delta) => {
                    let committed_account =
                        self.committed_accounts.get_mut(&account_update.account_id()).ok_or_else(
                            || anyhow::anyhow!("account delta in block for non-existent account"),
                        )?;
                    committed_account
                        .apply_delta(account_delta)
                        .context("failed to apply account delta")?;
                },
                // No state to keep for private accounts other than the commitment on the account
                // tree
                AccountUpdateDetails::Private => {},
            }
        }

        let notes_tree = proven_block.build_output_note_tree();
        for (block_note_index, created_note) in proven_block.output_notes() {
            let note_path = notes_tree.get_note_path(block_note_index);
            let note_inclusion_proof = NoteInclusionProof::new(
                proven_block.header().block_num(),
                block_note_index.leaf_index_value(),
                note_path,
            )
            .context("failed to construct note inclusion proof")?;

            if let OutputNote::Full(note) = created_note {
                self.committed_notes
                    .insert(note.id(), MockChainNote::Public(note.clone(), note_inclusion_proof));
            } else {
                self.committed_notes.insert(
                    created_note.id(),
                    MockChainNote::Private(
                        created_note.id(),
                        *created_note.metadata(),
                        note_inclusion_proof,
                    ),
                );
            }
        }

        debug_assert_eq!(
            self.chain.commitment(),
            proven_block.header().chain_commitment(),
            "current mock chain commitment and new block's chain commitment should match"
        );
        debug_assert_eq!(
            BlockNumber::from(self.chain.as_mmr().forest().num_leaves() as u32),
            proven_block.header().block_num(),
            "current mock chain length and new block's number should match"
        );

        self.chain.push(proven_block.header().commitment());
        self.blocks.push(proven_block);

        Ok(())
    }

    fn pending_transactions_to_batches(&mut self) -> anyhow::Result<Vec<ProvenBatch>> {
        // Batches must contain at least one transaction, so if there are no pending transactions,
        // return early.
        if self.pending_transactions.is_empty() {
            return Ok(vec![]);
        }

        let pending_transactions = core::mem::take(&mut self.pending_transactions);

        // TODO: Distribute the transactions into multiple batches if the transactions would not fit
        // into a single batch (according to max input notes, max output notes and max accounts).
        let proposed_batch = self.propose_transaction_batch(pending_transactions)?;
        let proven_batch = self.prove_transaction_batch(proposed_batch)?;

        Ok(vec![proven_batch])
    }

    fn apply_pending_objects_to_block(
        &mut self,
        proven_block: &mut ProvenBlock,
    ) -> anyhow::Result<()> {
        // Add pending accounts to block.
        let pending_account_updates = core::mem::take(&mut self.pending_objects.updated_accounts);

        let updated_accounts_block: BTreeSet<AccountId> = proven_block
            .updated_accounts()
            .iter()
            .map(|update| update.account_id())
            .collect();

        for (id, account_update) in pending_account_updates {
            if updated_accounts_block.contains(&id) {
                return Err(anyhow::anyhow!(
                    "account {id} is already modified in block through transactions",
                ));
            }

            self.account_tree
                .insert(id, account_update.final_state_commitment())
                .context("failed to insert pending account into tree")?;

            proven_block.updated_accounts_mut().push(account_update);
        }

        // Add pending nullifiers to block.
        let pending_created_nullifiers =
            core::mem::take(&mut self.pending_objects.created_nullifiers);

        let created_nullifiers_block: BTreeSet<Nullifier> =
            proven_block.created_nullifiers().iter().copied().collect();

        for nullifier in pending_created_nullifiers {
            if created_nullifiers_block.contains(&nullifier) {
                return Err(anyhow::anyhow!(
                    "nullifier {nullifier} is already created in block through transactions",
                ));
            }

            self.nullifier_tree
                .mark_spent(nullifier, proven_block.header().block_num())
                .context("failed to insert pending nullifier into tree")?;

            proven_block.created_nullifiers_mut().push(nullifier);
        }

        // Add pending output notes to block.
        let output_notes_block: BTreeSet<NoteId> =
            proven_block.output_notes().map(|(_, output_note)| output_note.id()).collect();

        // We could distribute notes over multiple batches (if space is available), but most likely
        // one is sufficient.
        if self.pending_objects.output_notes.len() > MAX_OUTPUT_NOTES_PER_BATCH {
            return Err(anyhow::anyhow!(
                "too many pending output notes: {}, max allowed: {MAX_OUTPUT_NOTES_PER_BATCH}",
                self.pending_objects.output_notes.len(),
            ));
        }

        let mut pending_note_batch = Vec::with_capacity(self.pending_objects.output_notes.len());
        let pending_output_notes = core::mem::take(&mut self.pending_objects.output_notes);
        for (note_idx, output_note) in pending_output_notes.into_iter().enumerate() {
            if output_notes_block.contains(&output_note.id()) {
                return Err(anyhow::anyhow!(
                    "output note {} is already created in block through transactions",
                    output_note.id()
                ));
            }

            pending_note_batch.push((note_idx, output_note));
        }

        if (proven_block.output_note_batches().len() + 1) > MAX_BATCHES_PER_BLOCK {
            return Err(anyhow::anyhow!(
                "too many batches in block: cannot add more pending notes".to_string(),
            ));
        }

        proven_block.output_note_batches_mut().push(pending_note_batch);

        let updated_block_note_tree = proven_block.build_output_note_tree().root();

        // Update account tree and nullifier tree root in the block.
        let block_header = proven_block.header();
        let updated_header = BlockHeader::new(
            block_header.version(),
            block_header.prev_block_commitment(),
            block_header.block_num(),
            block_header.chain_commitment(),
            self.account_tree.root(),
            self.nullifier_tree.root(),
            updated_block_note_tree,
            block_header.tx_commitment(),
            block_header.tx_kernel_commitment(),
            block_header.proof_commitment(),
            block_header.timestamp(),
        );
        proven_block.set_block_header(updated_header);

        Ok(())
    }

    /// Creates a new block in the mock chain.
    ///
    /// This will make all the objects currently pending available for use.
    ///
    /// If a `timestamp` is provided, it will be set on the block.
    ///
    /// Block building is divided into a few steps:
    ///
    /// 1. Build batches from pending transactions and a block from those batches. This results in a
    ///    block.
    /// 2. Take that block and apply only its account/nullifier tree updates to the chain.
    /// 3. Then take the pending objects and insert them directly into the proven block. This means
    ///    we have to update the header of the block as well, with the newly inserted pending
    ///    accounts/nullifiers/notes. This is why we already did step 2, so that we can insert the
    ///    pending objects directly into the account/nullifier tree to get the latest correct state
    ///    of those trees. Then take the root of the trees and update them in the header of the
    ///    block. This should be pretty efficient because we don't have to do any tree insertions
    ///    multiple times (which would be slow).
    /// 4. Finally, now the block contains both the updates from the regular transactions/batches as
    ///    well as the pending objects. Now insert all the remaining updates into the chain state.
    fn prove_block_inner(&mut self, timestamp: Option<u32>) -> anyhow::Result<ProvenBlock> {
        // Create batches from pending transactions.
        // ----------------------------------------------------------------------------------------

        let batches = self.pending_transactions_to_batches()?;

        // Create block.
        // ----------------------------------------------------------------------------------------

        let block_timestamp =
            timestamp.unwrap_or(self.latest_block_header().timestamp() + Self::TIMESTAMP_STEP_SECS);

        let proposed_block = self
            .propose_block_at(batches, block_timestamp)
            .context("failed to create proposed block")?;
        let mut proven_block = self.prove_block(proposed_block).context("failed to prove block")?;

        // We apply the block tree updates here, so that apply_pending_objects_to_block can easily
        // update the block header of this block with the pending accounts and nullifiers.
        self.apply_block_tree_updates(&proven_block)
            .context("failed to apply block tree updates")?;

        if !self.pending_objects.is_empty() {
            self.apply_pending_objects_to_block(&mut proven_block)?;
        }

        self.apply_block(proven_block.clone()).context("failed to apply block")?;

        Ok(proven_block)
    }
}

/// Creates the genesis state, consisting of a block containing the provided account updates and an
/// account tree with those accounts.
fn create_genesis_state(
    accounts: impl IntoIterator<Item = Account>,
) -> anyhow::Result<(ProvenBlock, AccountTree)> {
    let block_account_updates: Vec<BlockAccountUpdate> = accounts
        .into_iter()
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

    let output_note_batches = Vec::new();
    let created_nullifiers = Vec::new();
    let transactions = OrderedTransactionHeaders::new_unchecked(Vec::new());

    let version = 0;
    let prev_block_commitment = Word::empty();
    let block_num = BlockNumber::from(0u32);
    let chain_commitment = Blockchain::new().commitment();
    let account_root = account_tree.root();
    let nullifier_root = NullifierTree::new().root();
    let note_root = BlockNoteTree::empty().root();
    let tx_commitment = transactions.commitment();
    let tx_kernel_commitment = TransactionKernel::kernel_commitment();
    let proof_commitment = Word::empty();
    let timestamp = MockChain::TIMESTAMP_START_SECS;

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
        timestamp,
    );

    Ok((
        ProvenBlock::new_unchecked(
            header,
            block_account_updates,
            output_note_batches,
            created_nullifiers,
            transactions,
        ),
        account_tree,
    ))
}

impl Default for MockChain {
    fn default() -> Self {
        MockChain::new()
    }
}

// PENDING OBJECTS
// ================================================================================================

/// Aggregates all entities that were added using the _pending_ APIs of the [`MockChain`].
#[derive(Default, Debug, Clone)]
struct PendingObjects {
    /// Account updates for the block.
    updated_accounts: BTreeMap<AccountId, BlockAccountUpdate>,

    /// Note batches created in transactions in the block.
    output_notes: Vec<OutputNote>,

    /// Nullifiers produced in transactions in the block.
    created_nullifiers: Vec<Nullifier>,
}

impl PendingObjects {
    pub fn new() -> PendingObjects {
        PendingObjects {
            updated_accounts: BTreeMap::new(),
            output_notes: vec![],
            created_nullifiers: vec![],
        }
    }

    /// Returns `true` if there are no pending objects, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.updated_accounts.is_empty()
            && self.output_notes.is_empty()
            && self.created_nullifiers.is_empty()
    }
}

// ACCOUNT STATE
// ================================================================================================

/// Helper type for increased readability at call-sites. Indicates whether to build a new (nonce =
/// ZERO) or existing account (nonce = ONE).
pub enum AccountState {
    New,
    Exists,
}

// TX CONTEXT INPUT
// ================================================================================================

/// Helper type to abstract over the inputs to [`MockChain::build_tx_context`]. See that method's
/// docs for details.
#[derive(Debug, Clone)]
pub enum TxContextInput {
    AccountId(AccountId),
    Account(Account),
    ExecutedTransaction(Box<ExecutedTransaction>),
}

impl From<AccountId> for TxContextInput {
    fn from(account: AccountId) -> Self {
        Self::AccountId(account)
    }
}

impl From<Account> for TxContextInput {
    fn from(account: Account) -> Self {
        Self::Account(account)
    }
}

impl From<ExecutedTransaction> for TxContextInput {
    fn from(tx: ExecutedTransaction) -> Self {
        Self::ExecutedTransaction(Box::new(tx))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_objects::{
        account::{AccountStorage, AccountStorageMode},
        asset::FungibleAsset,
        testing::{
            account_component::AccountMockComponent,
            account_id::{ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_SENDER},
        },
    };

    use super::*;

    #[test]
    fn with_accounts() -> anyhow::Result<()> {
        let account = AccountBuilder::new([4; 32])
            .storage_mode(AccountStorageMode::Public)
            .with_auth_component(Auth::IncrNonce)
            .with_component(
                AccountMockComponent::new_with_slots(
                    TransactionKernel::testing_assembler(),
                    vec![AccountStorage::mock_item_2().slot],
                )
                .unwrap(),
            )
            .build_existing()?;

        let mock_chain = MockChain::with_accounts(&[account.clone()])?;

        assert_eq!(mock_chain.committed_account(account.id())?, &account);

        // Check that transaction inputs retrieved from the chain are against the block header with
        // the current account tree root.
        let tx_context = mock_chain.build_tx_context(account.id(), &[], &[])?.build()?;
        assert_eq!(tx_context.tx_inputs().block_header().block_num(), BlockNumber::from(0u32));
        assert_eq!(
            tx_context.tx_inputs().block_header().account_root(),
            mock_chain.account_tree.root()
        );

        Ok(())
    }

    #[test]
    fn prove_until_block() -> anyhow::Result<()> {
        let mut chain = MockChain::new();
        let block = chain.prove_until_block(5)?;
        assert_eq!(block.header().block_num(), 5u32.into());
        assert_eq!(chain.proven_blocks().len(), 6);

        Ok(())
    }

    #[test]
    fn private_account_state_update() -> anyhow::Result<()> {
        let faucet_id = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into()?;
        let account_builder = AccountBuilder::new([4; 32])
            .storage_mode(AccountStorageMode::Private)
            .with_component(BasicWallet);

        let mut mock_chain = MockChain::new();
        let account = mock_chain.add_pending_account_from_builder(
            Auth::BasicAuth,
            account_builder,
            AccountState::New,
        )?;
        let account_id = account.id();
        assert_eq!(account.nonce().as_int(), 0);

        let note_1 = mock_chain.add_pending_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            account.id(),
            &[Asset::Fungible(FungibleAsset::new(faucet_id, 1000u64).unwrap())],
            NoteType::Private,
        )?;

        mock_chain.prove_next_block()?;

        let tx = mock_chain
            .build_tx_context(TxContextInput::Account(account), &[], &[note_1])?
            .build()?
            .execute()?;

        mock_chain.add_pending_executed_transaction(&tx)?;
        mock_chain.prove_next_block()?;

        assert!(tx.final_account().nonce().as_int() > 0);
        assert_eq!(
            tx.final_account().commitment(),
            mock_chain.account_tree.open(account_id).state_commitment()
        );

        Ok(())
    }
}
