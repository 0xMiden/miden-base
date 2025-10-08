// TRANSACTION CONTEXT BUILDER
// ================================================================================================

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::Context;
use miden_lib::testing::account_component::IncrNonceAuthComponent;
use miden_lib::testing::mock_account::MockAccountExt;
use miden_objects::EMPTY_WORD;
use miden_objects::account::{
    Account,
    AccountHeader,
    AccountId,
    PartialAccount,
    PartialStorage,
    PartialStorageMap,
    PublicKeyCommitment,
    Signature,
    StorageSlot,
};
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::assembly::debuginfo::SourceManagerSync;
use miden_objects::asset::PartialVault;
use miden_objects::note::{Note, NoteId};
use miden_objects::testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE;
use miden_objects::testing::noop_auth_component::NoopAuthComponent;
use miden_objects::transaction::{
    AccountInputs,
    OutputNote,
    TransactionArgs,
    TransactionInputs,
    TransactionScript,
};
use miden_objects::vm::AdviceMap;
use miden_processor::{AdviceInputs, Felt, Word};
use miden_tx::TransactionMastStore;
use miden_tx::auth::BasicAuthenticator;
use rand_chacha::ChaCha20Rng;

use super::TransactionContext;
use crate::{MockChain, MockChainNote};

pub type MockAuthenticator = BasicAuthenticator<ChaCha20Rng>;

// TRANSACTION CONTEXT BUILDER
// ================================================================================================

/// [TransactionContextBuilder] is a utility to construct [TransactionContext] for testing
/// purposes. It allows users to build accounts, create notes, provide advice inputs, and
/// execute code. The VM process can be inspected afterward.
///
/// # Examples
///
/// Create a new account and execute code:
/// ```
/// # use miden_testing::TransactionContextBuilder;
/// # use miden_objects::{account::AccountBuilder,Felt, FieldElement};
/// # use miden_lib::transaction::TransactionKernel;
/// let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();
///
/// let code = "
/// use.$kernel::prologue
/// use.mock::account
///
/// begin
///     exec.prologue::prepare_transaction
///     push.5
///     swap drop
/// end
/// ";
///
/// let process = tx_context.execute_code(code).unwrap();
/// assert_eq!(process.stack.get(0), Felt::new(5),);
/// ```
pub struct TransactionContextBuilder {
    source_manager: Arc<dyn SourceManagerSync>,
    account: Account,
    advice_inputs: AdviceInputs,
    authenticator: Option<MockAuthenticator>,
    expected_output_notes: Vec<Note>,
    foreign_account_inputs: BTreeMap<AccountId, AccountInputs>,
    input_notes: Vec<Note>,
    tx_script: Option<TransactionScript>,
    tx_script_args: Word,
    note_args: BTreeMap<NoteId, Word>,
    transaction_inputs: Option<TransactionInputs>,
    auth_args: Word,
    signatures: Vec<(PublicKeyCommitment, Word, Signature)>,
    is_lazy_loading_enabled: bool,
}

impl TransactionContextBuilder {
    pub fn new(account: Account) -> Self {
        Self {
            source_manager: Arc::new(DefaultSourceManager::default()),
            account,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            tx_script: None,
            tx_script_args: EMPTY_WORD,
            authenticator: None,
            advice_inputs: Default::default(),
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: BTreeMap::new(),
            auth_args: EMPTY_WORD,
            signatures: Vec::new(),
            is_lazy_loading_enabled: false,
        }
    }

    /// Initializes a [TransactionContextBuilder] with a mock account.
    ///
    /// The wallet:
    ///
    /// - Includes a series of mocked assets ([miden_objects::asset::AssetVault::mock()]).
    /// - Has a nonce of `1` (so it does not imply seed validation).
    /// - Has an ID of [`ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE`].
    /// - Has an account code based on an
    ///   [miden_lib::testing::account_component::MockAccountComponent].
    pub fn with_existing_mock_account() -> Self {
        Self::new(Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            IncrNonceAuthComponent,
        ))
    }

    /// Same as [`Self::with_existing_mock_account`] but with a [`NoopAuthComponent`].
    pub fn with_noop_auth_account() -> Self {
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, NoopAuthComponent);

        Self::new(account)
    }

    /// Initializes a [TransactionContextBuilder] with a mocked fungible faucet.
    pub fn with_fungible_faucet(acct_id: u128, initial_balance: Felt) -> Self {
        let account = Account::mock_fungible_faucet(acct_id, initial_balance);
        Self::new(account)
    }

    /// Initializes a [TransactionContextBuilder] with a mocked non-fungible faucet.
    pub fn with_non_fungible_faucet(acct_id: u128) -> Self {
        let account = Account::mock_non_fungible_faucet(acct_id);
        Self::new(account)
    }

    /// Extend the advice inputs with the provided [AdviceInputs] instance.
    pub fn extend_advice_inputs(mut self, advice_inputs: AdviceInputs) -> Self {
        self.advice_inputs.extend(advice_inputs);
        self
    }

    /// Extend the advice inputs map with the provided iterator.
    pub fn extend_advice_map(
        mut self,
        map_entries: impl IntoIterator<Item = (Word, Vec<Felt>)>,
    ) -> Self {
        self.advice_inputs.map.extend(map_entries);
        self
    }

    /// Set the authenticator for the transaction (if needed)
    pub fn authenticator(mut self, authenticator: Option<MockAuthenticator>) -> Self {
        self.authenticator = authenticator;
        self
    }

    /// Set foreign account codes that are used by the transaction
    pub fn foreign_accounts(mut self, inputs: impl IntoIterator<Item = AccountInputs>) -> Self {
        self.foreign_account_inputs
            .extend(inputs.into_iter().map(|account_inputs| (account_inputs.id(), account_inputs)));
        self
    }

    /// Extend the set of used input notes
    pub fn extend_input_notes(mut self, input_notes: Vec<Note>) -> Self {
        self.input_notes.extend(input_notes);
        self
    }

    /// Set the desired transaction script
    pub fn tx_script(mut self, tx_script: TransactionScript) -> Self {
        self.tx_script = Some(tx_script);
        self
    }

    /// Set the transaction script arguments
    pub fn tx_script_args(mut self, tx_script_args: Word) -> Self {
        self.tx_script_args = tx_script_args;
        self
    }

    /// Set the desired auth arguments
    pub fn auth_args(mut self, auth_args: Word) -> Self {
        self.auth_args = auth_args;
        self
    }

    /// Set the desired transaction inputs
    pub fn tx_inputs(mut self, tx_inputs: TransactionInputs) -> Self {
        assert_eq!(
            AccountHeader::from(&self.account),
            tx_inputs.account().into(),
            "account in context and account provided via tx inputs are not the same account"
        );
        self.transaction_inputs = Some(tx_inputs);
        self
    }

    /// Causes the transaction to only construct a minimal partial account as the transaction
    /// input, causing lazy loading of assets and storage map items throughout transaction
    /// execution. Additionally, foreign accounts aren't provided via the transaction args but are
    /// lazy loaded as well.
    ///
    /// This exists to test lazy loading selectively and should go away in the future.
    pub fn enable_lazy_loading(mut self) -> Self {
        self.is_lazy_loading_enabled = true;
        self
    }

    /// Extend the note arguments map with the provided one.
    pub fn extend_note_args(mut self, note_args: BTreeMap<NoteId, Word>) -> Self {
        self.note_args.extend(note_args);
        self
    }

    /// Extend the expected output notes.
    pub fn extend_expected_output_notes(mut self, output_notes: Vec<OutputNote>) -> Self {
        let output_notes = output_notes.into_iter().filter_map(|n| match n {
            OutputNote::Full(note) => Some(note),
            OutputNote::Partial(_) => None,
            OutputNote::Header(_) => None,
        });

        self.expected_output_notes.extend(output_notes);
        self
    }

    /// Sets the [`SourceManagerSync`] on the [`TransactionContext`] that will be built.
    ///
    /// This source manager should contain the sources of all involved scripts and account code in
    /// order to provide better error messages if an error occurs.
    pub fn with_source_manager(mut self, source_manager: Arc<dyn SourceManagerSync>) -> Self {
        self.source_manager = source_manager.clone();
        self
    }

    /// Add a new signature for the message and the public key.
    pub fn add_signature(
        mut self,
        pub_key: PublicKeyCommitment,
        message: Word,
        signature: Signature,
    ) -> Self {
        self.signatures.push((pub_key, message, signature));
        self
    }

    /// Builds the [TransactionContext].
    ///
    /// If no transaction inputs were provided manually, an ad-hoc MockChain is created in order
    /// to generate valid block data for the required notes.
    pub fn build(self) -> anyhow::Result<TransactionContext> {
        let tx_inputs = match self.transaction_inputs {
            Some(tx_inputs) => tx_inputs,
            None => {
                // If no specific transaction inputs was provided, initialize an ad-hoc mockchain
                // to generate valid block header/MMR data

                let mut builder = MockChain::builder();
                for i in self.input_notes {
                    builder.add_output_note(OutputNote::Full(i));
                }
                let mut mock_chain = builder.build()?;

                mock_chain.prove_next_block().context("failed to prove first block")?;
                mock_chain.prove_next_block().context("failed to prove second block")?;

                let input_note_ids: Vec<NoteId> =
                    mock_chain.committed_notes().values().map(MockChainNote::id).collect();

                mock_chain
                    .get_transaction_inputs(&self.account, &input_note_ids, &[])
                    .context("failed to get transaction inputs from mock chain")?
            },
        };

        // If partial loading is enabled, construct an account that doesn't contain all
        // merkle paths of assets and storage maps, in order to test lazy loading.
        // Otherwise, load the full account.
        let tx_inputs = if self.is_lazy_loading_enabled {
            let (_account, block_header, partial_blockchain, input_notes) = tx_inputs.into_parts();
            // Note that we use self.account instead of account, because we cannot do the same
            // operation on a partial vault.
            let account = minimal_partial_account(&self.account)?;

            TransactionInputs::new(account, block_header, partial_blockchain, input_notes)?
        } else {
            tx_inputs
        };

        let foreign_account_inputs = if self.is_lazy_loading_enabled {
            Vec::new()
        } else {
            self.foreign_account_inputs.values().cloned().collect()
        };

        let tx_args = TransactionArgs::new(AdviceMap::default(), foreign_account_inputs)
            .with_note_args(self.note_args);

        let mut tx_args = if let Some(tx_script) = self.tx_script {
            tx_args.with_tx_script_and_args(tx_script, self.tx_script_args)
        } else {
            tx_args
        };

        tx_args = tx_args.with_auth_args(self.auth_args);

        tx_args.extend_advice_inputs(self.advice_inputs.clone());
        tx_args.extend_output_note_recipients(self.expected_output_notes.clone());

        for (public_key_commitment, message, signature) in self.signatures {
            tx_args.add_signature(public_key_commitment, message, signature);
        }

        let mast_store = {
            let mast_forest_store = TransactionMastStore::new();
            mast_forest_store.load_account_code(tx_inputs.account().code());

            for acc_inputs in self.foreign_account_inputs.values() {
                mast_forest_store.insert(acc_inputs.code().mast());
            }

            mast_forest_store
        };

        Ok(TransactionContext {
            account: self.account,
            expected_output_notes: self.expected_output_notes,
            foreign_account_inputs: self.foreign_account_inputs,
            tx_args,
            tx_inputs,
            mast_store,
            authenticator: self.authenticator,
            advice_inputs: self.advice_inputs,
            source_manager: self.source_manager,
        })
    }
}

impl Default for TransactionContextBuilder {
    fn default() -> Self {
        Self::with_existing_mock_account()
    }
}

/// Creates a minimal [`PartialAccount`] from the provided full [`Account`].
fn minimal_partial_account(account: &Account) -> anyhow::Result<PartialAccount> {
    // Construct a partial vault that tracks the empty word, but none of the assets
    // that are actually in the asset tree. That way, the partial vault has the same
    // root as the full vault, but will not add any relevant merkle paths to the
    // merkle store, which will test lazy loading of assets.
    let mut partial_vault = PartialVault::default();
    partial_vault.add(account.vault().open(Word::empty().into()))?;

    // Construct a partial storage that tracks the empty word in all storage maps, but none
    // of the other keys, following the same rationale as the partial vault above.
    let storage_header = account.storage().to_header();
    let storage_maps =
        account.storage().slots().iter().filter_map(|storage_slot| match storage_slot {
            StorageSlot::Map(storage_map) => {
                let mut partial_storage_map = PartialStorageMap::default();
                let key = Word::empty();
                let witness = storage_map.open(&key);
                partial_storage_map
                    .add(witness)
                    .expect("adding the first proof should never error");
                Some(partial_storage_map)
            },
            _ => None,
        });
    let partial_storage = PartialStorage::new(storage_header, storage_maps)
        .expect("provided storage maps should match storage header");

    let account = PartialAccount::new(
        account.id(),
        account.nonce(),
        account.code().clone(),
        partial_storage,
        partial_vault,
        None,
    )?;

    Ok(account)
}
