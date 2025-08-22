// TRANSACTION CONTEXT BUILDER
// ================================================================================================

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::Context;
use miden_lib::testing::account_component::IncrNonceAuthComponent;
use miden_lib::testing::mock_account::MockAccountExt;
use miden_lib::transaction::TransactionKernel;
use miden_objects::EMPTY_WORD;
use miden_objects::account::Account;
use miden_objects::assembly::debuginfo::SourceManagerSync;
use miden_objects::assembly::{Assembler, default_source_manager_arc_dyn};
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
use miden_tx::TransactionMastStore;
use miden_tx::auth::BasicAuthenticator;
use rand_chacha::ChaCha20Rng;
use vm_processor::{AdviceInputs, Felt, Word};

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
    assembler: Assembler,
    source_manager: Arc<dyn SourceManagerSync>,
    account: Account,
    account_seed: Option<Word>,
    advice_inputs: AdviceInputs,
    authenticator: Option<MockAuthenticator>,
    expected_output_notes: Vec<Note>,
    foreign_account_inputs: Vec<AccountInputs>,
    input_notes: Vec<Note>,
    tx_script: Option<TransactionScript>,
    tx_script_args: Word,
    note_args: BTreeMap<NoteId, Word>,
    transaction_inputs: Option<TransactionInputs>,
    auth_args: Word,
}

impl TransactionContextBuilder {
    pub fn new(account: Account) -> Self {
        let source_manager = default_source_manager_arc_dyn();
        Self {
            assembler: TransactionKernel::with_mock_libraries(source_manager.clone()),
            source_manager,
            account,
            account_seed: None,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            tx_script: None,
            tx_script_args: EMPTY_WORD,
            authenticator: None,
            advice_inputs: Default::default(),
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
            auth_args: EMPTY_WORD,
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
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, IncrNonceAuthComponent);

        let source_manager = default_source_manager_arc_dyn();
        let assembler = TransactionKernel::with_mock_libraries(source_manager.clone());
        Self {
            assembler,
            source_manager,
            account,
            account_seed: None,
            authenticator: None,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            advice_inputs: Default::default(),
            tx_script: None,
            tx_script_args: EMPTY_WORD,
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
            auth_args: EMPTY_WORD,
        }
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

    /// Returns a clone of the assembler.
    ///
    /// This is primarily useful to assemble a script whose source will end up in the source manager
    /// that is passed to the processor. This will help generate better error messages.
    pub fn assembler(&self) -> Assembler {
        self.assembler.clone()
    }

    /// Override and set the account seed manually
    pub fn account_seed(mut self, account_seed: Option<Word>) -> Self {
        self.account_seed = account_seed;
        self
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
    pub fn foreign_accounts(mut self, inputs: Vec<AccountInputs>) -> Self {
        self.foreign_account_inputs = inputs;
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
        self.transaction_inputs = Some(tx_inputs);
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

    pub fn with_source_manager(mut self, source_manager: Arc<dyn SourceManagerSync>) -> Self {
        // TODO <https://github.com/0xMiden/miden-base/issues/1779>
        self.source_manager = source_manager.clone();
        self.assembler = TransactionKernel::with_mock_libraries(source_manager.clone());
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

                let mut mock_chain = MockChain::default();
                for i in self.input_notes {
                    mock_chain.add_pending_note(OutputNote::Full(i));
                }

                mock_chain.prove_next_block().context("failed to prove first block")?;
                mock_chain.prove_next_block().context("failed to prove second block")?;

                let input_note_ids: Vec<NoteId> =
                    mock_chain.committed_notes().values().map(MockChainNote::id).collect();

                mock_chain
                    .get_transaction_inputs(
                        self.account.clone(),
                        self.account_seed,
                        &input_note_ids,
                        &[],
                    )
                    .context("failed to get transaction inputs from mock chain")?
            },
        };

        let tx_args = TransactionArgs::new(AdviceMap::default(), self.foreign_account_inputs)
            .with_note_args(self.note_args);

        let mut tx_args = if let Some(tx_script) = self.tx_script {
            tx_args.with_tx_script_and_args(tx_script, self.tx_script_args)
        } else {
            tx_args
        };

        tx_args = tx_args.with_auth_args(self.auth_args);

        tx_args.extend_advice_inputs(self.advice_inputs.clone());
        tx_args.extend_output_note_recipients(self.expected_output_notes.clone());

        let mast_store = {
            let mast_forest_store = TransactionMastStore::new();
            mast_forest_store.load_account_code(tx_inputs.account().code());

            for acc_inputs in tx_args.foreign_account_inputs() {
                mast_forest_store.insert(acc_inputs.code().mast());
            }

            mast_forest_store
        };

        Ok(TransactionContext {
            expected_output_notes: self.expected_output_notes,
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
