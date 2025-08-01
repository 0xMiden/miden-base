use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::ToString,
    vec::Vec,
};

use anyhow::Context;
use miden_block_prover::{LocalBlockProver, ProvenBlockError};
use miden_lib::note::{create_p2id_note, create_p2ide_note};
use miden_objects::{
    MAX_BATCHES_PER_BLOCK, MAX_OUTPUT_NOTES_PER_BATCH, NoteError,
    account::{Account, AccountId, AuthSecretKey, StorageSlot, delta::AccountUpdateDetails},
    asset::Asset,
    batch::{ProposedBatch, ProvenBatch},
    block::{
        AccountTree, AccountWitness, BlockHeader, BlockInputs, BlockNumber, Blockchain,
        NullifierTree, NullifierWitness, ProposedBlock, ProvenBlock,
    },
    crypto::merkle::SmtProof,
    note::{Note, NoteHeader, NoteId, NoteInclusionProof, NoteType, Nullifier},
    transaction::{
        AccountInputs, ExecutedTransaction, InputNote, InputNotes, OrderedTransactionHeaders,
        OutputNote, PartialBlockchain, ProvenTransaction, TransactionHeader, TransactionInputs,
    },
};
use miden_tx::{
    auth::BasicAuthenticator,
    utils::{ByteReader, Deserializable, Serializable},
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use vm_processor::{DeserializationError, Word, crypto::RpoRandomCoin};
use winterfell::ByteWriter;

use super::note::MockChainNote;
use crate::{MockChainBuilder, ProvenTransactionExt, TransactionContextBuilder};

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
/// # use miden_objects::{Felt, asset::{Asset, FungibleAsset}, note::NoteType};
/// # use miden_testing::{Auth, MockChain, TransactionContextBuilder};
///
/// # fn main() -> Result<()> {
/// let mut builder = MockChain::builder();
///
/// let faucet = builder.create_new_faucet(Auth::BasicAuth, "USDT", 100_000)?;
/// let asset = Asset::from(FungibleAsset::new(faucet.id(), 10)?);
///
/// let sender = builder.create_new_wallet(Auth::BasicAuth)?;
/// let target = builder.create_new_wallet(Auth::BasicAuth)?;
///
/// let note = builder.add_p2id_note(faucet.id(), target.id(), &[asset], NoteType::Public)?;
///
/// let mock_chain = builder.build()?;
///
/// // The target account is a new account so we move it into the build_tx_context, since the
/// // chain's committed accounts do not yet contain it.
/// let tx_context = mock_chain.build_tx_context(target, &[note.id()], &[])?.build()?;
/// let executed_transaction = tx_context.execute()?;
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
/// let mut builder = MockChain::builder();
///
/// // Add a recipient wallet.
/// let receiver = builder.add_existing_wallet(Auth::BasicAuth)?;
///
/// // Add a wallet with assets.
/// let sender = builder.add_existing_wallet(Auth::IncrNonce)?;
/// let fungible_asset = FungibleAsset::mock(10).unwrap_fungible();
///
/// // Add a pending P2ID note to the chain.
/// let note = builder.add_p2id_note(
///     sender.id(),
///     receiver.id(),
///     &[Asset::Fungible(fungible_asset)],
///     NoteType::Public,
/// )?;
///
/// let mut mock_chain = builder.build()?;
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

    /// Note batches created in transactions in the block.
    ///
    /// These will become available once the block is proven.
    pending_output_notes: Vec<OutputNote>,

    /// Transactions that have been submitted to the chain but have not yet been included in a
    /// block.
    pending_transactions: Vec<ProvenTransaction>,

    /// NoteID |-> MockChainNote mapping to simplify note retrieval.
    committed_notes: BTreeMap<NoteId, MockChainNote>,

    /// AccountId |-> Account mapping to simplify transaction creation. Latest known account
    /// state is maintained for each account here.
    ///
    /// The map always holds the most recent *public* state known for every account. For private
    /// accounts, however, transactions do not emit the post-transaction state, so their entries
    /// remain at the last observed state.
    committed_accounts: BTreeMap<AccountId, Account>,

    /// AccountId |-> AccountCredentials mapping to store the seed and authenticator for accounts
    /// to simplify transaction creation.
    account_credentials: BTreeMap<AccountId, AccountCredentials>,

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
        Self::builder().build().expect("empty chain should be valid")
    }

    /// Returns a new, empty [`MockChainBuilder`].
    pub fn builder() -> MockChainBuilder {
        MockChainBuilder::new()
    }

    /// Creates a new `MockChain` with the provided genesis block and account tree.
    pub(super) fn from_genesis_block(
        genesis_block: ProvenBlock,
        account_tree: AccountTree,
        account_credentials: BTreeMap<AccountId, AccountCredentials>,
    ) -> anyhow::Result<Self> {
        let mut chain = MockChain {
            chain: Blockchain::default(),
            blocks: vec![],
            nullifier_tree: NullifierTree::default(),
            account_tree,
            pending_output_notes: Vec::new(),
            pending_transactions: Vec::new(),
            committed_notes: BTreeMap::new(),
            committed_accounts: BTreeMap::new(),
            account_credentials,
            // Initialize RNG with default seed.
            rng: ChaCha20Rng::from_seed(Default::default()),
        };

        // We do not have to apply the tree changes, because the account tree is already initialized
        // and the nullifier tree is empty at genesis.
        chain
            .apply_block(genesis_block)
            .context("failed to build account from builder")?;

        debug_assert_eq!(chain.blocks.len(), 1);
        debug_assert_eq!(chain.committed_accounts.len(), chain.account_tree.num_accounts());

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

        self.selective_partial_blockchain(latest_block_header.block_num(), reference_blocks)
    }

    /// Creates a new [`PartialBlockchain`] with all reference blocks in the given iterator except
    /// for the reference block header in the chain and returns that reference block header.
    ///
    /// The intended use for the reference block header is to become the reference block of a new
    /// transaction batch or block.
    pub fn selective_partial_blockchain(
        &self,
        reference_block: BlockNumber,
        reference_blocks: impl IntoIterator<Item = BlockNumber>,
    ) -> anyhow::Result<(BlockHeader, PartialBlockchain)> {
        let reference_block_header = self.block_header(reference_block.as_usize());
        // Deduplicate block numbers so each header will be included just once. This is required so
        // PartialBlockchain::from_blockchain does not panic.
        let reference_blocks: BTreeSet<_> = reference_blocks.into_iter().collect();

        // Include all block headers except the reference block itself.
        let mut block_headers = Vec::new();

        for block_ref_num in &reference_blocks {
            let block_index = block_ref_num.as_usize();
            let block = self
                .blocks
                .get(block_index)
                .ok_or_else(|| anyhow::anyhow!("block {} not found in chain", block_ref_num))?;
            let block_header = block.header().clone();
            // Exclude the reference block header.
            if block_header.commitment() != reference_block_header.commitment() {
                block_headers.push(block_header);
            }
        }

        let partial_blockchain =
            PartialBlockchain::from_blockchain_at(&self.chain, reference_block, block_headers)?;

        Ok((reference_block_header, partial_blockchain))
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

    /// Initializes a [`TransactionContextBuilder`] for executing against a specific block number.
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
    pub fn build_tx_context_at(
        &self,
        reference_block: impl Into<BlockNumber>,
        input: impl Into<TxContextInput>,
        note_ids: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionContextBuilder> {
        let input = input.into();
        let reference_block = reference_block.into();

        let credentials = self.account_credentials.get(&input.id());
        let authenticator =
            credentials.and_then(|credentials| credentials.authenticator().cloned());
        let seed = credentials.and_then(|credentials| credentials.seed());

        anyhow::ensure!(
            reference_block.as_usize() < self.blocks.len(),
            "reference block {reference_block} is out of range (latest {})",
            self.latest_block_header().block_num()
        );

        let account = match input {
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
            TxContextInput::Account(account) => account,
            TxContextInput::ExecutedTransaction(executed_transaction) => {
                let mut initial_account = executed_transaction.initial_account().clone();
                initial_account
                    .apply_delta(executed_transaction.account_delta())
                    .context("could not apply delta from previous transaction")?;

                initial_account
            },
        };

        let tx_inputs = self
            .get_transaction_inputs_at(
                reference_block,
                account.clone(),
                seed,
                note_ids,
                unauthenticated_notes,
            )
            .context("failed to gather transaction inputs")?;

        let tx_context_builder = TransactionContextBuilder::new(account)
            .authenticator(authenticator)
            .account_seed(seed)
            .tx_inputs(tx_inputs);

        Ok(tx_context_builder)
    }

    /// Initializes a [`TransactionContextBuilder`] for executing against the last block header.
    ///
    /// This is a wrapper around [`Self::build_tx_context_at`] which uses the latest block as the
    /// reference block. See that function's docs for details.
    pub fn build_tx_context(
        &self,
        input: impl Into<TxContextInput>,
        note_ids: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionContextBuilder> {
        let reference_block = self.latest_block_header().block_num();
        self.build_tx_context_at(reference_block, input, note_ids, unauthenticated_notes)
    }

    // INPUTS APIS
    // ----------------------------------------------------------------------------------------

    /// Returns a valid [`TransactionInputs`] for the specified entities, executing against a
    /// specific block number.
    pub fn get_transaction_inputs_at(
        &self,
        reference_block: BlockNumber,
        account: Account,
        account_seed: Option<Word>,
        notes: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionInputs> {
        let ref_block = self.block_header(reference_block.as_usize());

        let mut input_notes = vec![];
        let mut block_headers_map: BTreeMap<BlockNumber, BlockHeader> = BTreeMap::new();
        for note in notes {
            let input_note: InputNote = self
                .committed_notes
                .get(note)
                .with_context(|| format!("note with id {note} not found"))?
                .clone()
                .try_into()
                .with_context(|| {
                    format!("failed to convert mock chain note with id {note} into input note")
                })?;

            let note_block_num = input_note
                .location()
                .with_context(|| format!("note location not available: {note}"))?
                .block_num();

            if note_block_num > ref_block.block_num() {
                anyhow::bail!(
                    "note with ID {note} was created in block {note_block_num} which is larger than the reference block number {}",
                    ref_block.block_num()
                )
            }

            if note_block_num != ref_block.block_num() {
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

        let block_headers = block_headers_map.values();
        let (_, partial_blockchain) = self.selective_partial_blockchain(
            reference_block,
            block_headers.map(BlockHeader::block_num),
        )?;

        let input_notes = InputNotes::new(input_notes)?;

        Ok(TransactionInputs::new(
            account,
            account_seed,
            ref_block.clone(),
            partial_blockchain,
            input_notes,
        )?)
    }

    /// Returns a valid [`TransactionInputs`] for the specified entities.
    pub fn get_transaction_inputs(
        &self,
        account: Account,
        account_seed: Option<Word>,
        notes: &[NoteId],
        unauthenticated_notes: &[Note],
    ) -> anyhow::Result<TransactionInputs> {
        let latest_block_num = self.latest_block_header().block_num();
        self.get_transaction_inputs_at(
            latest_block_num,
            account,
            account_seed,
            notes,
            unauthenticated_notes,
        )
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
        self.pending_output_notes.push(note);
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

    /// Adds a P2IDE [`OutputNote`] (pay‑to‑ID‑extended) to the list of pending notes.
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

    // PRIVATE HELPERS
    // ----------------------------------------------------------------------------------------

    /// Applies the given block to the chain state, which means:
    ///
    /// - Insert account and nullifiers into the respective trees.
    /// - Updated accounts from the block are updated in the committed accounts.
    /// - Created notes are inserted into the committed notes.
    /// - Consumed notes are removed from the committed notes.
    /// - The block is appended to the [`BlockChain`] and the list of proven blocks.
    fn apply_block(&mut self, proven_block: ProvenBlock) -> anyhow::Result<()> {
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

        for account_update in proven_block.updated_accounts() {
            match account_update.details() {
                AccountUpdateDetails::New(account) => {
                    self.committed_accounts.insert(account.id(), account.clone());
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
            let note_path = notes_tree.open(block_note_index);
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

    fn apply_pending_notes_to_block(
        &mut self,
        proven_block: &mut ProvenBlock,
    ) -> anyhow::Result<()> {
        // Add pending output notes to block.
        let output_notes_block: BTreeSet<NoteId> =
            proven_block.output_notes().map(|(_, output_note)| output_note.id()).collect();

        // We could distribute notes over multiple batches (if space is available), but most likely
        // one is sufficient.
        if self.pending_output_notes.len() > MAX_OUTPUT_NOTES_PER_BATCH {
            return Err(anyhow::anyhow!(
                "too many pending output notes: {}, max allowed: {MAX_OUTPUT_NOTES_PER_BATCH}",
                self.pending_output_notes.len(),
            ));
        }

        let mut pending_note_batch = Vec::with_capacity(self.pending_output_notes.len());
        let pending_output_notes = core::mem::take(&mut self.pending_output_notes);
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

        // Update note tree root in the block header.
        let block_header = proven_block.header();
        let updated_header = BlockHeader::new(
            block_header.version(),
            block_header.prev_block_commitment(),
            block_header.block_num(),
            block_header.chain_commitment(),
            block_header.account_root(),
            block_header.nullifier_root(),
            updated_block_note_tree,
            block_header.tx_commitment(),
            block_header.tx_kernel_commitment(),
            block_header.proof_commitment(),
            block_header.fee_parameters().clone(),
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
    /// 2. Take the pending notes and insert them directly into the proven block. This means we have
    ///    to update the header of the block with the updated block note tree root.
    /// 3. Finally, the block contains both the updates from the regular transactions/batches as
    ///    well as the pending notes. Now insert all the accounts, nullifier and notes into the
    ///    chain state.
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

        if !self.pending_output_notes.is_empty() {
            self.apply_pending_notes_to_block(&mut proven_block)?;
        }

        self.apply_block(proven_block.clone()).context("failed to apply block")?;

        Ok(proven_block)
    }
}

impl Default for MockChain {
    fn default() -> Self {
        MockChain::new()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for MockChain {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.chain.write_into(target);
        self.blocks.write_into(target);
        self.nullifier_tree.write_into(target);
        self.account_tree.write_into(target);
        self.pending_output_notes.write_into(target);
        self.pending_transactions.write_into(target);
        self.committed_accounts.write_into(target);
        self.committed_notes.write_into(target);
        self.account_credentials.write_into(target);
    }
}

impl Deserializable for MockChain {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let chain = Blockchain::read_from(source)?;
        let blocks = Vec::<ProvenBlock>::read_from(source)?;
        let nullifier_tree = NullifierTree::read_from(source)?;
        let account_tree = AccountTree::read_from(source)?;
        let pending_output_notes = Vec::<OutputNote>::read_from(source)?;
        let pending_transactions = Vec::<ProvenTransaction>::read_from(source)?;
        let committed_accounts = BTreeMap::<AccountId, Account>::read_from(source)?;
        let committed_notes = BTreeMap::<NoteId, MockChainNote>::read_from(source)?;
        let account_credentials = BTreeMap::<AccountId, AccountCredentials>::read_from(source)?;

        Ok(Self {
            chain,
            blocks,
            nullifier_tree,
            account_tree,
            pending_output_notes,
            pending_transactions,
            committed_notes,
            committed_accounts,
            account_credentials,
            rng: ChaCha20Rng::from_os_rng(),
        })
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

// ACCOUNT CREDENTIALS
// ================================================================================================

/// A wrapper around the seed and authenticator of an account.
#[derive(Debug, Clone)]
pub(super) struct AccountCredentials {
    seed: Option<Word>,
    authenticator: Option<BasicAuthenticator<ChaCha20Rng>>,
}

impl AccountCredentials {
    pub fn new(seed: Option<Word>, authenticator: Option<BasicAuthenticator<ChaCha20Rng>>) -> Self {
        Self { seed, authenticator }
    }

    pub fn authenticator(&self) -> Option<&BasicAuthenticator<ChaCha20Rng>> {
        self.authenticator.as_ref()
    }

    pub fn seed(&self) -> Option<Word> {
        self.seed
    }
}

impl PartialEq for AccountCredentials {
    fn eq(&self, other: &Self) -> bool {
        let authenticator_eq = match (&self.authenticator, &other.authenticator) {
            (Some(a), Some(b)) => {
                a.keys().keys().zip(b.keys().keys()).all(|(a_key, b_key)| a_key == b_key)
            },
            (None, None) => true,
            _ => false,
        };
        authenticator_eq && self.seed == other.seed
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountCredentials {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.seed.write_into(target);
        self.authenticator
            .as_ref()
            .map(|auth| auth.keys().iter().collect::<Vec<_>>())
            .write_into(target);
    }
}

impl Deserializable for AccountCredentials {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let seed = Option::<Word>::read_from(source)?;
        let authenticator = Option::<Vec<(Word, AuthSecretKey)>>::read_from(source)?;

        let authenticator = authenticator
            .map(|keys| BasicAuthenticator::new_with_rng(&keys, ChaCha20Rng::from_os_rng()));

        Ok(Self { seed, authenticator })
    }
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

impl TxContextInput {
    /// Returns the account ID that this input references.
    fn id(&self) -> AccountId {
        match self {
            TxContextInput::AccountId(account_id) => *account_id,
            TxContextInput::Account(account) => account.id(),
            TxContextInput::ExecutedTransaction(executed_transaction) => {
                executed_transaction.account_id()
            },
        }
    }
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
    use miden_lib::account::wallets::BasicWallet;
    use miden_objects::{
        account::{AccountBuilder, AccountStorageMode},
        asset::FungibleAsset,
        testing::account_id::{
            ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
            ACCOUNT_ID_SENDER,
        },
    };

    use super::*;
    use crate::Auth;

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

        let mut builder = MockChain::builder();
        let account = builder.add_account_from_builder(
            Auth::BasicAuth,
            account_builder,
            AccountState::New,
        )?;

        let account_id = account.id();
        assert_eq!(account.nonce().as_int(), 0);

        let note_1 = builder.add_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            account.id(),
            &[Asset::Fungible(FungibleAsset::new(faucet_id, 1000u64).unwrap())],
            NoteType::Private,
        )?;

        let mut mock_chain = builder.build()?;
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

    #[test]
    fn mock_chain_serialization() {
        let mut builder = MockChain::builder();

        let mut notes = vec![];
        for i in 0..10 {
            let account = builder
                .add_account_from_builder(
                    Auth::BasicAuth,
                    AccountBuilder::new([i; 32]).with_component(BasicWallet),
                    AccountState::New,
                )
                .unwrap();
            let note = builder
                .add_p2id_note(
                    ACCOUNT_ID_SENDER.try_into().unwrap(),
                    account.id(),
                    &[Asset::Fungible(
                        FungibleAsset::new(
                            ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET.try_into().unwrap(),
                            1000u64,
                        )
                        .unwrap(),
                    )],
                    NoteType::Private,
                )
                .unwrap();
            notes.push((account, note));
        }

        let mut chain = builder.build().unwrap();
        for (account, note) in notes {
            let tx = chain
                .build_tx_context(TxContextInput::Account(account), &[], &[note])
                .unwrap()
                .build()
                .unwrap()
                .execute()
                .unwrap();
            chain.add_pending_executed_transaction(&tx).unwrap();
            chain.prove_next_block().unwrap();
        }

        let bytes = chain.to_bytes();

        let deserialized = MockChain::read_from_bytes(&bytes).unwrap();

        assert_eq!(chain.chain.as_mmr().peaks(), deserialized.chain.as_mmr().peaks());
        assert_eq!(chain.blocks, deserialized.blocks);
        assert_eq!(chain.nullifier_tree, deserialized.nullifier_tree);
        assert_eq!(chain.account_tree, deserialized.account_tree);
        assert_eq!(chain.pending_output_notes, deserialized.pending_output_notes);
        assert_eq!(chain.pending_transactions, deserialized.pending_transactions);
        assert_eq!(chain.committed_accounts, deserialized.committed_accounts);
        assert_eq!(chain.committed_notes, deserialized.committed_notes);
        assert_eq!(chain.account_credentials, deserialized.account_credentials);
    }
}
