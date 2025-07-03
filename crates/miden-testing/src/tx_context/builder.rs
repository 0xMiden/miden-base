// TRANSACTION CONTEXT BUILDER
// ================================================================================================

use alloc::{collections::BTreeMap, vec::Vec};

use miden_lib::{transaction::TransactionKernel, utils::word_to_masm_push_string};
use miden_objects::{
    EMPTY_WORD, FieldElement,
    account::{Account, AccountId},
    assembly::Assembler,
    asset::{Asset, FungibleAsset, NonFungibleAsset},
    note::{Note, NoteExecutionHint, NoteId, NoteType},
    testing::{
        account_component::{IncrNonceAuthComponent, NoopAuthComponent},
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3, ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            ACCOUNT_ID_SENDER,
        },
        constants::{
            CONSUMED_ASSET_1_AMOUNT, CONSUMED_ASSET_2_AMOUNT, CONSUMED_ASSET_3_AMOUNT,
            NON_FUNGIBLE_ASSET_DATA_2,
        },
        note::NoteBuilder,
        storage::prepare_assets,
    },
    transaction::{
        AccountInputs, OutputNote, TransactionArgs, TransactionInputs, TransactionScript,
    },
    vm::AdviceMap,
};
use miden_tx::{TransactionMastStore, auth::BasicAuthenticator};
use rand::{Rng, SeedableRng};
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
/// let tx_context = TransactionContextBuilder::with_standard_account(Felt::ONE).build();
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
    rng: ChaCha20Rng,
}

impl TransactionContextBuilder {
    pub fn new(account: Account) -> Self {
        Self {
            assembler: TransactionKernel::testing_assembler_with_mock_account(),
            account,
            account_seed: None,
            input_notes: Vec::new(),
            expected_output_notes: Vec::new(),
            rng: ChaCha20Rng::from_seed([0_u8; 32]),
            tx_script: None,
            tx_script_arg: EMPTY_WORD,
            authenticator: None,
            advice_inputs: Default::default(),
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
        }
    }

    /// Initializes a [TransactionContextBuilder] with a mocked standard wallet.
    pub fn with_standard_account(nonce: Felt) -> Self {
        // Build standard account with normal assembler because the testing one already contains it
        let assembler = TransactionKernel::testing_assembler();
        let auth_component = IncrNonceAuthComponent::new(assembler.clone()).unwrap().into();

        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            nonce,
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
            rng: ChaCha20Rng::from_seed([0_u8; 32]),
            tx_script: None,
            tx_script_arg: EMPTY_WORD,
            transaction_inputs: None,
            note_args: BTreeMap::new(),
            foreign_account_inputs: vec![],
        }
    }

    pub fn with_noop_auth_account(nonce: Felt) -> Self {
        let assembler = TransactionKernel::testing_assembler();
        let auth_component = NoopAuthComponent::new(assembler.clone()).unwrap().into();

        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            nonce,
            auth_component,
            assembler,
        );

        Self { account, ..Self::default() }
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

    /// Creates a new output [Note] for the transaction corresponding to this context.
    fn add_output_note(
        &mut self,
        inputs: impl IntoIterator<Item = Felt>,
        assets: impl IntoIterator<Item = Asset>,
    ) -> Note {
        let note = NoteBuilder::new(self.account.id(), &mut self.rng)
            .note_inputs(inputs)
            .expect("The inputs should be valid")
            .add_assets(assets)
            .build(&self.assembler)
            .expect("The note details should be valid");

        self.expected_output_notes.push(note.clone());
        note
    }

    /// Add a note from a [NoteBuilder]
    fn input_note_simple(
        &mut self,
        sender: AccountId,
        assets: impl IntoIterator<Item = Asset>,
        inputs: impl IntoIterator<Item = Felt>,
    ) -> Note {
        NoteBuilder::new(sender, ChaCha20Rng::from_seed(self.rng.random()))
            .note_inputs(inputs)
            .unwrap()
            .add_assets(assets)
            .build(&self.assembler)
            .unwrap()
    }

    /// Adds one input note with a note script that creates another output note.
    fn input_note_with_one_output_note(
        &mut self,
        sender: AccountId,
        assets: impl IntoIterator<Item = Asset>,
        inputs: impl IntoIterator<Item = Felt>,
        output: &Note,
    ) -> Note {
        let code = format!(
            "
            use.miden::contracts::wallets::basic->wallet
            use.test::account

            begin
                # NOTE
                # ---------------------------------------------------------------------------------
                padw padw
                push.{recipient}
                push.{execution_hint_always}
                push.{PUBLIC_NOTE}
                push.{aux}
                push.{tag}
                # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8)]

                call.wallet::create_note
                # => [note_idx, pad(15)]

                push.{asset}
                call.account::add_asset_to_note
                # => [ASSET, note_idx, pad(15)]

                # clear the stack
                repeat.5 dropw end
                # => []
            end
            ",
            PUBLIC_NOTE = NoteType::Public as u8,
            recipient = word_to_masm_push_string(&output.recipient().digest()),
            aux = output.metadata().aux(),
            tag = output.metadata().tag(),
            asset = prepare_assets(output.assets())[0],
            execution_hint_always = Felt::from(NoteExecutionHint::always())
        );

        NoteBuilder::new(sender, ChaCha20Rng::from_seed(self.rng.random()))
            .note_inputs(inputs)
            .unwrap()
            .add_assets(assets)
            .code(code)
            .build(&self.assembler)
            .unwrap()
    }

    /// Adds one input note with a note script that creates 2 output notes.
    fn input_note_with_two_output_notes(
        &mut self,
        sender: AccountId,
        inputs: impl IntoIterator<Item = Felt>,
        output0: &Note,
        output1: &Note,
        asset: Asset,
    ) -> Note {
        let code = format!(
            "
            use.miden::contracts::wallets::basic->wallet
            use.test::account

            begin

                # NOTE 0
                # ---------------------------------------------------------------------------------
                padw padw
                push.{recipient0}
                push.{execution_hint_always}
                push.{PUBLIC_NOTE}
                push.{aux0}
                push.{tag0}
                # => [tag_0, aux_0, note_type, execution_hint, RECIPIENT_0, pad(8)]

                call.wallet::create_note
                # => [note_idx_0, pad(15)]

                push.{asset0}
                call.account::add_asset_to_note
                # => [ASSET_0, note_idx_0, pad(15)]
                
                dropw dropw dropw
                # => [pad(8)]

                # NOTE 1
                # ---------------------------------------------------------------------------------
                push.{recipient1}
                push.{execution_hint_always}
                push.{PUBLIC_NOTE}
                push.{aux1}
                push.{tag1}
                # => [tag_1, aux_1, note_type, execution_hint, RECIPIENT_1, pad(8)]

                call.wallet::create_note
                # => [note_idx_1, pad(15)]
                
                push.{asset1}
                call.account::add_asset_to_note
                # => [ASSET_1, note_idx_1, pad(15)]

                repeat.5 dropw end
            end
            ",
            PUBLIC_NOTE = NoteType::Public as u8,
            recipient0 = word_to_masm_push_string(&output0.recipient().digest()),
            aux0 = output0.metadata().aux(),
            tag0 = output0.metadata().tag(),
            asset0 = prepare_assets(output0.assets())[0],
            recipient1 = word_to_masm_push_string(&output1.recipient().digest()),
            aux1 = output1.metadata().aux(),
            tag1 = output1.metadata().tag(),
            asset1 = prepare_assets(output1.assets())[0],
            execution_hint_always = Felt::from(NoteExecutionHint::always())
        );

        NoteBuilder::new(sender, ChaCha20Rng::from_seed(self.rng.random()))
            .note_inputs(inputs)
            .unwrap()
            .add_assets([asset])
            .code(code)
            .build(&self.assembler)
            .unwrap()
    }

    fn input_note_transfer(
        &mut self,
        sender: AccountId,
        assets: impl IntoIterator<Item = Asset>,
    ) -> Note {
        let code = "
            use.miden::note
            use.miden::contracts::wallets::basic->wallet

            begin
                # read the assets to memory
                push.0 exec.note::get_assets
                # => [num_assets, dest_ptr]

                # assert the number of assets is 3
                push.3 assert_eq
                # => [dest_ptr]

                # add the first asset to the vault
                padw dup.4 mem_loadw 
                # => [ASSET, dest_ptr]

                # pad the stack before call
                padw swapw padw padw swapdw
                # => [ASSET, pad(12), dest_ptr]
                
                # add the first asset to the vault
                call.wallet::receive_asset dropw movup.12
                # => [dest_ptr, pad(12)]

                # add the second asset to the vault
                add.4 dup movdn.13
                # => [dest_ptr+4, pad(12), dest_ptr+4]

                # load the asset
                padw movup.4 mem_loadw
                # => [ASSET, pad(12), dest_ptr+4]

                # add the second asset to the vault
                call.wallet::receive_asset dropw movup.12
                # => [dest_ptr+4, pad(12)]

                # add the third asset to the vault
                add.4 padw movup.4 mem_loadw
                # => [ASSET, pad(12)]
                
                call.wallet::receive_asset
                dropw dropw dropw dropw
                # => []
            end
        ";

        NoteBuilder::new(sender, ChaCha20Rng::from_seed(self.rng.random()))
            .add_assets(assets)
            .code(code)
            .build(&self.assembler)
            .unwrap()
    }

    /// Adds a set of input notes that output notes where inputs are smaller than needed and
    /// do not add up to match the output.
    pub fn with_mock_notes_too_few_input(mut self) -> Self {
        // ACCOUNT IDS
        // --------------------------------------------------------------------------------------------
        let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap();
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap();

        // ASSETS
        // --------------------------------------------------------------------------------------------
        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT).unwrap().into();
        let fungible_asset_2: Asset =
            FungibleAsset::new(faucet_id_2, CONSUMED_ASSET_2_AMOUNT).unwrap().into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT).unwrap().into();

        let output_note0 = self.add_output_note([1u32.into()], [fungible_asset_1]);
        let output_note1 = self.add_output_note([2u32.into()], [fungible_asset_2]);

        // expected by `output_notes_data_procedure`
        let _output_note2 = self.add_output_note([3u32.into()], [fungible_asset_3]);

        let input_note1 = self.input_note_with_two_output_notes(
            sender,
            [1u32.into()],
            &output_note0,
            &output_note1,
            fungible_asset_1,
        );

        self.extend_input_notes(vec![input_note1])
    }

    /// Adds a set of input notes that output notes in an asset-preserving manner.
    pub fn with_mock_notes_preserved(mut self) -> Self {
        // ACCOUNT IDS
        // --------------------------------------------------------------------------------------------
        let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap();
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap();

        // ASSETS
        // --------------------------------------------------------------------------------------------
        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT).unwrap().into();
        let fungible_asset_2: Asset =
            FungibleAsset::new(faucet_id_2, CONSUMED_ASSET_2_AMOUNT).unwrap().into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT).unwrap().into();

        let output_note0 = self.add_output_note([1u32.into()], [fungible_asset_1]);
        let output_note1 = self.add_output_note([2u32.into()], [fungible_asset_2]);
        let output_note2 = self.add_output_note([3u32.into()], [fungible_asset_3]);

        let input_note1 = self.input_note_with_two_output_notes(
            sender,
            [1u32.into()],
            &output_note0,
            &output_note1,
            fungible_asset_1,
        );
        let input_note2 = self.input_note_with_one_output_note(
            sender,
            [fungible_asset_2, fungible_asset_3],
            [1u32.into()],
            &output_note2,
        );

        self.extend_input_notes(vec![input_note1, input_note2])
    }

    pub fn with_mock_notes_preserved_with_account_vault_delta(mut self) -> Self {
        // ACCOUNT IDS
        // --------------------------------------------------------------------------------------------
        let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap();
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap();

        // ASSETS
        // --------------------------------------------------------------------------------------------
        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT).unwrap().into();
        let fungible_asset_2: Asset =
            FungibleAsset::new(faucet_id_2, CONSUMED_ASSET_2_AMOUNT).unwrap().into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT).unwrap().into();
        let nonfungible_asset_1: Asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);

        let output_note0 = self.add_output_note([1u32.into()], [fungible_asset_1]);
        let output_note1 = self.add_output_note([2u32.into()], [fungible_asset_2]);
        let output_note2 = self.add_output_note([3u32.into()], [fungible_asset_3]);

        let input_note1 = self.input_note_with_two_output_notes(
            sender,
            [1u32.into()],
            &output_note0,
            &output_note1,
            fungible_asset_1,
        );
        let input_note2 = self.input_note_with_one_output_note(
            sender,
            [fungible_asset_2, fungible_asset_3],
            [1u32.into()],
            &output_note2,
        );

        let input_note5 = self
            .input_note_transfer(sender, [fungible_asset_1, fungible_asset_3, nonfungible_asset_1]);

        self.extend_input_notes(vec![input_note1, input_note2, input_note5])
    }

    pub fn with_mock_notes_too_many_fungible_input(mut self) -> Self {
        // ACCOUNT IDS
        // --------------------------------------------------------------------------------------------
        let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap();
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap();

        // ASSETS
        // --------------------------------------------------------------------------------------------
        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT).unwrap().into();
        let fungible_asset_2: Asset =
            FungibleAsset::new(faucet_id_2, CONSUMED_ASSET_2_AMOUNT).unwrap().into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT).unwrap().into();

        let output_note0 = self.add_output_note([1u32.into()], [fungible_asset_1]);
        let output_note1 = self.add_output_note([2u32.into()], [fungible_asset_2]);
        let output_note2 = self.add_output_note([3u32.into()], [fungible_asset_3]);

        let input_note1 = self.input_note_with_two_output_notes(
            sender,
            [1u32.into()],
            &output_note0,
            &output_note1,
            fungible_asset_1,
        );
        let input_note2 = self.input_note_with_one_output_note(
            sender,
            [fungible_asset_2, fungible_asset_3],
            [1u32.into()],
            &output_note2,
        );
        let input_note3 =
            self.input_note_simple(sender, [fungible_asset_2, fungible_asset_3], [2u32.into()]);

        self.extend_input_notes(vec![input_note1, input_note2, input_note3])
    }

    pub fn with_mock_notes_too_many_non_fungible_input(mut self) -> Self {
        // ACCOUNT IDS
        // --------------------------------------------------------------------------------------------
        let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2).unwrap();
        let faucet_id_3 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_3).unwrap();

        // ASSETS
        // --------------------------------------------------------------------------------------------
        let fungible_asset_1: Asset =
            FungibleAsset::new(faucet_id_1, CONSUMED_ASSET_1_AMOUNT).unwrap().into();
        let fungible_asset_2: Asset =
            FungibleAsset::new(faucet_id_2, CONSUMED_ASSET_2_AMOUNT).unwrap().into();
        let fungible_asset_3: Asset =
            FungibleAsset::new(faucet_id_3, CONSUMED_ASSET_3_AMOUNT).unwrap().into();
        let nonfungible_asset_1: Asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);

        let output_note0 = self.add_output_note([1u32.into()], [fungible_asset_1]);
        let output_note1 = self.add_output_note([2u32.into()], [fungible_asset_2]);
        let output_note2 = self.add_output_note([3u32.into()], [fungible_asset_3]);

        let input_note1 = self.input_note_with_two_output_notes(
            sender,
            [1u32.into()],
            &output_note0,
            &output_note1,
            fungible_asset_1,
        );
        let input_note2 = self.input_note_with_one_output_note(
            sender,
            [fungible_asset_2, fungible_asset_3],
            [1u32.into()],
            &output_note2,
        );
        let input_note4 = self.input_note_simple(sender, [nonfungible_asset_1], [1u32.into()]);

        self.extend_input_notes(vec![input_note1, input_note2, input_note4])
    }

    /// Builds the [TransactionContext].
    ///
    /// If no transaction inputs were provided manually, an ad-hoc MockChain is created in order
    /// to generate valid block data for the required notes.
    pub fn build(self) -> TransactionContext {
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

                mock_chain.prove_next_block().unwrap();
                mock_chain.prove_next_block().unwrap();

                let input_note_ids: Vec<NoteId> =
                    mock_chain.committed_notes().values().map(MockChainNote::id).collect();

                mock_chain
                    .get_transaction_inputs(
                        self.account.clone(),
                        self.account_seed,
                        &input_note_ids,
                        &[],
                    )
                    .expect("failed to get transaction inputs from mock chain")
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

        TransactionContext {
            expected_output_notes: self.expected_output_notes,
            tx_args,
            tx_inputs,
            mast_store,
            authenticator: self.authenticator,
            advice_inputs: self.advice_inputs,
            source_manager,
        }
    }
}

impl Default for TransactionContextBuilder {
    fn default() -> Self {
        Self::with_standard_account(Felt::ZERO)
    }
}
