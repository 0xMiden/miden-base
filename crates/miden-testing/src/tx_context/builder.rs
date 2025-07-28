// TRANSACTION CONTEXT BUILDER
// ================================================================================================

use alloc::{collections::BTreeMap, vec::Vec};

use anyhow::Context;
use miden_lib::{account::auth::AuthNone, transaction::TransactionKernel};
use miden_objects::{
    EMPTY_WORD, FieldElement,
    account::{Account, AccountComponent},
    assembly::Assembler,
    note::{Note, NoteId},
    testing::{
        account_component::NoopAuthComponent,
        account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    },
    transaction::{
        AccountInputs, OutputNote, TransactionArgs, TransactionInputs, TransactionScript,
    },
    vm::AdviceMap,
};
use miden_tx::{TransactionMastStore, auth::BasicAuthenticator};
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
/// use.kernel::prologue
/// use.test::account
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
    account: Account,
    account_seed: Option<Word>,
    advice_inputs: AdviceInputs,
    authenticator: Option<MockAuthenticator>,
    expected_output_notes: Vec<Note>,
    foreign_account_inputs: Vec<AccountInputs>,
    input_notes: Vec<Note>,
    tx_script: Option<TransactionScript>,
    tx_script_arg: Word,
    note_args: BTreeMap<NoteId, Word>,
    transaction_inputs: Option<TransactionInputs>,
}

impl TransactionContextBuilder {
    pub fn new(account: Account) -> Self {
        Self {
            assembler: TransactionKernel::testing_assembler_with_mock_account(),
            account,
            account_seed: None,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            tx_script: None,
            tx_script_arg: EMPTY_WORD,
            authenticator: None,
            advice_inputs: Default::default(),
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
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
    ///   [miden_objects::testing::account_component::AccountMockComponent].
    pub fn with_existing_mock_account() -> Self {
        // Build standard account with normal assembler because the testing one already contains it
        let assembler = TransactionKernel::testing_assembler();
        let auth_component: AccountComponent = AuthNone.into();

        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            auth_component,
            assembler,
        );

        let assembler = TransactionKernel::testing_assembler_with_mock_account();

        Self {
            assembler: assembler.clone(),
            account,
            account_seed: None,
            authenticator: None,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            advice_inputs: Default::default(),
            tx_script: None,
            tx_script_arg: EMPTY_WORD,
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
        }
    }

    pub fn with_noop_auth_account(nonce: Felt) -> Self {
        let assembler = TransactionKernel::testing_assembler();
        let auth_component = NoopAuthComponent::new(assembler.clone()).expect("valid component");

        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            nonce,
            auth_component,
            assembler,
        );

        Self::new(account)
    }

    /// Initializes a [TransactionContextBuilder] with a mocked fungible faucet.
    pub fn with_fungible_faucet(acct_id: u128, nonce: Felt, initial_balance: Felt) -> Self {
        let account = Account::mock_fungible_faucet(
            acct_id,
            nonce,
            initial_balance,
            TransactionKernel::testing_assembler(),
        );

        Self { account, ..Self::default() }
    }

    /// Initializes a [TransactionContextBuilder] with a mocked non-fungible faucet.
    pub fn with_non_fungible_faucet(acct_id: u128, nonce: Felt, empty_reserved_slot: bool) -> Self {
        let account = Account::mock_non_fungible_faucet(
            acct_id,
            nonce,
            empty_reserved_slot,
            TransactionKernel::testing_assembler(),
        );

        Self { account, ..Self::default() }
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
        self.advice_inputs
            .extend_map(map_entries.into_iter().map(|(hash, input)| (hash.into(), input)));
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

    /// Set the transaction script argument
    pub fn tx_script_arg(mut self, tx_script_arg: Word) -> Self {
        self.tx_script_arg = tx_script_arg;
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

    /// Builds the [TransactionContext].
    ///
    /// If no transaction inputs were provided manually, an ad-hoc MockChain is created in order
    /// to generate valid block data for the required notes.
    pub fn build(self) -> anyhow::Result<TransactionContext> {
        let source_manager = self.assembler.source_manager();

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
            tx_args.with_tx_script_and_arg(tx_script, self.tx_script_arg)
        } else {
            tx_args
        };

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
            source_manager,
        })
    }
}

impl Default for TransactionContextBuilder {
    fn default() -> Self {
        Self::with_existing_mock_account()
    }
}
