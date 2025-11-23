use alloc::sync::Arc;

use anyhow::Context;
use assert_matches::assert_matches;
use miden_lib::AuthScheme;
use miden_lib::account::interface::AccountInterface;
use miden_lib::account::wallets::BasicWallet;
use miden_lib::note::create_p2id_note;
use miden_lib::testing::account_component::IncrNonceAuthComponent;
use miden_lib::testing::mock_account::MockAccountExt;
use miden_lib::transaction::TransactionKernel;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountCode,
    AccountComponent,
    AccountStorage,
    AccountStorageMode,
    AccountType,
    StorageSlot,
};
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset};
use miden_objects::block::BlockNumber;
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteExecutionMode,
    NoteHeader,
    NoteId,
    NoteMetadata,
    NoteRecipient,
    NoteStorage,
    NoteTag,
    NoteType,
};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_PRIVATE_SENDER,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    ACCOUNT_ID_SENDER,
};
use miden_objects::testing::constants::{FUNGIBLE_ASSET_AMOUNT, NON_FUNGIBLE_ASSET_DATA};
use miden_objects::testing::note::DEFAULT_NOTE_CODE;
use miden_objects::transaction::{
    InputNotes,
    OutputNote,
    OutputNotes,
    TransactionArgs,
    TransactionSummary,
};
use miden_objects::{Felt, FieldElement, Hasher, ONE, Word};
use miden_processor::crypto::RpoRandomCoin;
use miden_tx::auth::UnreachableAuth;
use miden_tx::{TransactionExecutor, TransactionExecutorError};

use crate::kernel_tests::tx::ExecutionOutputExt;
use crate::utils::{create_public_p2any_note, create_spawn_note};
use crate::{Auth, MockChain, TransactionContextBuilder};

/// Tests that consuming a note created in a block that is newer than the reference block of the
/// transaction fails.
#[tokio::test]
async fn consuming_note_created_in_future_block_fails() -> anyhow::Result<()> {
    // Create a chain with an account
    let mut builder = MockChain::builder();
    let asset = FungibleAsset::mock(400);
    let account1 = builder.add_existing_wallet_with_assets(Auth::BasicAuth, [asset])?;
    let account2 = builder.add_existing_wallet_with_assets(Auth::BasicAuth, [asset])?;
    let output_note = create_public_p2any_note(account1.id(), [asset]);
    let spawn_note = builder.add_spawn_note([&output_note])?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_until_block(10u32)?;

    // Consume the spawn note which creates a note for account 2 to consume. It will be contained in
    // block 11. We use account 1 for this, so that account 2 remains unchanged and is still valid
    // against reference block 1 which we'll use for the later transaction.
    let tx = mock_chain
        .build_tx_context(account1.id(), &[spawn_note.id()], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .build()?
        .execute()
        .await?;

    // Add the transaction to the mock chain's mempool so it will be included in the next block.
    mock_chain.add_pending_executed_transaction(&tx)?;
    // Create block 11.
    mock_chain.prove_next_block()?;

    // Get the input note and assert that the note was created after block 11.
    let input_note = mock_chain.get_public_note(&output_note.id()).expect("note not found");
    assert_eq!(input_note.location().unwrap().block_num().as_u32(), 11);

    mock_chain.prove_next_block()?;
    mock_chain.prove_next_block()?;

    // Attempt to execute a transaction against reference block 1 with the note created in block 11
    // - which should fail.
    let tx_context = mock_chain.build_tx_context(account2.id(), &[], &[])?.build()?;

    let tx_executor = TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context)
        .with_source_manager(tx_context.source_manager());

    // Try to execute with block_ref==1
    let error = tx_executor
        .execute_transaction(
            account2.id(),
            BlockNumber::from(1),
            InputNotes::new(vec![input_note]).unwrap(),
            TransactionArgs::default(),
        )
        .await;

    assert_matches::assert_matches!(
        error,
        Err(TransactionExecutorError::NoteBlockPastReferenceBlock(..))
    );

    Ok(())
}

// BLOCK TESTS
// ================================================================================================

#[tokio::test]
async fn test_block_procedures() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let code = "
        use.miden::tx
        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # get the block data
            exec.tx::get_block_number
            exec.tx::get_block_timestamp
            exec.tx::get_block_commitment
            # => [BLOCK_COMMITMENT, block_timestamp, block_number]

            # truncate the stack
            swapdw dropw dropw
        end
        ";

    let exec_output = &tx_context.execute_code(code).await?;

    assert_eq!(
        exec_output.get_stack_word_be(0),
        tx_context.tx_inputs().block_header().commitment(),
        "top word on the stack should be equal to the block header commitment"
    );

    assert_eq!(
        exec_output.get_stack_element(4).as_int(),
        tx_context.tx_inputs().block_header().timestamp() as u64,
        "fifth element on the stack should be equal to the timestamp of the last block creation"
    );

    assert_eq!(
        exec_output.get_stack_element(5).as_int(),
        tx_context.tx_inputs().block_header().block_num().as_u64(),
        "sixth element on the stack should be equal to the block number"
    );
    Ok(())
}

#[tokio::test]
async fn executed_transaction_output_notes() -> anyhow::Result<()> {
    let executor_account =
        Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, IncrNonceAuthComponent);
    let account_id = executor_account.id();

    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);

    let combined_asset = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let removed_asset_4 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT / 2,
        )
        .expect("asset is valid"),
    );

    let tag1 = NoteTag::from_account_id(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
    );
    let tag2 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let tag3 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let aux1 = Felt::new(27);
    let aux2 = Felt::new(28);
    let aux3 = Felt::new(29);

    let note_type1 = NoteType::Private;
    let note_type2 = NoteType::Public;
    let note_type3 = NoteType::Public;

    tag1.validate(note_type1).expect("note tag 1 should support private notes");
    tag2.validate(note_type2).expect("note tag 2 should support public notes");
    tag3.validate(note_type3).expect("note tag 3 should support public notes");

    // In this test we create 3 notes. Note 1 is private, Note 2 is public and Note 3 is public
    // without assets.

    // Create the expected output note for Note 2 which is public
    let serial_num_2 = Word::from([1, 2, 3, 4u32]);
    let note_script_2 = ScriptBuilder::default().compile_note_script(DEFAULT_NOTE_CODE)?;
    let inputs_2 = NoteStorage::new(vec![ONE])?;
    let metadata_2 =
        NoteMetadata::new(account_id, note_type2, tag2, NoteExecutionHint::none(), aux2)?;
    let vault_2 = NoteAssets::new(vec![removed_asset_3, removed_asset_4])?;
    let recipient_2 = NoteRecipient::new(serial_num_2, note_script_2, inputs_2);
    let expected_output_note_2 = Note::new(vault_2, metadata_2, recipient_2);

    // Create the expected output note for Note 3 which is public
    let serial_num_3 = Word::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]);
    let note_script_3 = ScriptBuilder::default().compile_note_script(DEFAULT_NOTE_CODE)?;
    let inputs_3 = NoteStorage::new(vec![ONE, Felt::new(2)])?;
    let metadata_3 = NoteMetadata::new(
        account_id,
        note_type3,
        tag3,
        NoteExecutionHint::on_block_slot(1, 2, 3),
        aux3,
    )?;
    let vault_3 = NoteAssets::new(vec![])?;
    let recipient_3 = NoteRecipient::new(serial_num_3, note_script_3, inputs_3);
    let expected_output_note_3 = Note::new(vault_3, metadata_3, recipient_3);

    let tx_script_src = format!(
        "\
        use.miden::contracts::wallets::basic->wallet
        use.miden::output_note

        # Inputs:  [tag, aux, note_type, execution_hint, RECIPIENT]
        # Outputs: [note_idx]
        proc.create_note
            # pad the stack before the call to prevent accidental modification of the deeper stack
            # elements
            padw padw swapdw
            # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8)]

            call.output_note::create
            # => [note_idx, pad(15)]

            # remove excess PADs from the stack
            swapdw dropw dropw movdn.7 dropw drop drop drop
            # => [note_idx]
        end

        # Inputs:  [ASSET, note_idx]
        # Outputs: [ASSET, note_idx]
        proc.move_asset_to_note
            # pad the stack before call
            push.0.0.0 movdn.7 movdn.7 movdn.7 padw padw swapdw
            # => [ASSET, note_idx, pad(11)]

            call.wallet::move_asset_to_note
            # => [ASSET, note_idx, pad(11)]

            # remove excess PADs from the stack
            swapdw dropw dropw swapw movdn.7 drop drop drop
            # => [ASSET, note_idx]
        end

        ## TRANSACTION SCRIPT
        ## ========================================================================================
        begin
            ## Send some assets from the account vault
            ## ------------------------------------------------------------------------------------
            # partially deplete fungible asset balance
            push.0.1.2.3                        # recipient
            push.{EXECUTION_HINT_1}             # note execution hint
            push.{NOTETYPE1}                    # note_type
            push.{aux1}                         # aux
            push.{tag1}                         # tag
            exec.create_note
            # => [note_idx]

            push.{REMOVED_ASSET_1}              # asset_1
            # => [ASSET, note_idx]

            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_2}              # asset_2
            exec.move_asset_to_note dropw drop
            # => []

            # send non-fungible asset
            push.{RECIPIENT2}                   # recipient
            push.{EXECUTION_HINT_2}             # note execution hint
            push.{NOTETYPE2}                    # note_type
            push.{aux2}                         # aux
            push.{tag2}                         # tag
            exec.create_note
            # => [note_idx]

            push.{REMOVED_ASSET_3}              # asset_3
            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_4}              # asset_4
            exec.move_asset_to_note dropw drop
            # => []

            # create a public note without assets
            push.{RECIPIENT3}                   # recipient
            push.{EXECUTION_HINT_3}             # note execution hint
            push.{NOTETYPE3}                    # note_type
            push.{aux3}                         # aux
            push.{tag3}                         # tag
            exec.create_note drop
            # => []
        end
    ",
        REMOVED_ASSET_1 = Word::from(removed_asset_1),
        REMOVED_ASSET_2 = Word::from(removed_asset_2),
        REMOVED_ASSET_3 = Word::from(removed_asset_3),
        REMOVED_ASSET_4 = Word::from(removed_asset_4),
        RECIPIENT2 = expected_output_note_2.recipient().digest(),
        RECIPIENT3 = expected_output_note_3.recipient().digest(),
        NOTETYPE1 = note_type1 as u8,
        NOTETYPE2 = note_type2 as u8,
        NOTETYPE3 = note_type3 as u8,
        EXECUTION_HINT_1 = Felt::from(NoteExecutionHint::always()),
        EXECUTION_HINT_2 = Felt::from(NoteExecutionHint::none()),
        EXECUTION_HINT_3 = Felt::from(NoteExecutionHint::on_block_slot(11, 22, 33)),
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(tx_script_src)?;

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness

    let tx_context = TransactionContextBuilder::new(executor_account)
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![
            OutputNote::Full(expected_output_note_2.clone()),
            OutputNote::Full(expected_output_note_3.clone()),
        ])
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    // output notes
    // --------------------------------------------------------------------------------------------
    let output_notes = executed_transaction.output_notes();

    // check the total number of notes
    assert_eq!(output_notes.num_notes(), 3);

    // assert that the expected output note 1 is present
    let resulting_output_note_1 = executed_transaction.output_notes().get_note(0);

    let expected_recipient_1 = Word::from([0, 1, 2, 3u32]);
    let expected_note_assets_1 = NoteAssets::new(vec![combined_asset])?;
    let expected_note_id_1 = NoteId::new(expected_recipient_1, expected_note_assets_1.commitment());
    assert_eq!(resulting_output_note_1.id(), expected_note_id_1);

    // assert that the expected output note 2 is present
    let resulting_output_note_2 = executed_transaction.output_notes().get_note(1);

    let expected_note_id_2 = expected_output_note_2.id();
    let expected_note_metadata_2 = expected_output_note_2.metadata();
    assert_eq!(
        NoteHeader::from(resulting_output_note_2),
        NoteHeader::new(expected_note_id_2, *expected_note_metadata_2)
    );

    // assert that the expected output note 3 is present and has no assets
    let resulting_output_note_3 = executed_transaction.output_notes().get_note(2);

    assert_eq!(expected_output_note_3.id(), resulting_output_note_3.id());
    assert_eq!(expected_output_note_3.assets(), resulting_output_note_3.assets().unwrap());

    // make sure that the note storage length remains the same
    let resulting_note_2_recipient =
        resulting_output_note_2.recipient().expect("output note 2 is not full");
    assert_eq!(
        resulting_note_2_recipient.storage().num_items(),
        expected_output_note_2.storage().num_items()
    );

    let resulting_note_3_recipient =
        resulting_output_note_3.recipient().expect("output note 3 is not full");
    assert_eq!(
        resulting_note_3_recipient.storage().num_items(),
        expected_output_note_3.storage().num_items()
    );

    Ok(())
}

/// Tests that a transaction consuming and creating one note can emit an abort event in its auth
/// component to result in a [`TransactionExecutorError::Unauthorized`] error.
#[tokio::test]
async fn user_code_can_abort_transaction_with_summary() -> anyhow::Result<()> {
    let source_code = r#"
      use.miden::auth
      use.miden::tx
      const.AUTH_UNAUTHORIZED_EVENT=event("miden::auth::unauthorized")
      #! Inputs:  [AUTH_ARGS, pad(12)]
      #! Outputs: [pad(16)]
      export.auth_abort_tx
          dropw
          # => [pad(16)]

          push.0.0 exec.tx::get_block_number
          exec.::miden::native_account::incr_nonce
          # => [[final_nonce, block_num, 0, 0], pad(16)]
          # => [SALT, pad(16)]

          exec.auth::create_tx_summary
          # => [SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT]

          exec.auth::adv_insert_hqword

          exec.auth::hash_tx_summary
          # => [MESSAGE, pad(16)]

          emit.AUTH_UNAUTHORIZED_EVENT
      end
    "#;

    let auth_component =
        AccountComponent::compile(source_code, TransactionKernel::assembler(), vec![])
            .context("failed to compile auth component")?
            .with_supports_all_types();

    let account = AccountBuilder::new([42; 32])
        .storage_mode(AccountStorageMode::Private)
        .with_auth_component(auth_component)
        .with_component(BasicWallet)
        .build_existing()
        .context("failed to build account")?;

    // Consume and create a note so the input and outputs notes commitment is not the empty word.
    let mut rng = RpoRandomCoin::new(Word::empty());
    let output_note = create_p2id_note(
        account.id(),
        account.id(),
        vec![],
        NoteType::Private,
        Felt::ZERO,
        &mut rng,
    )?;
    let input_note = create_spawn_note(vec![&output_note])?;

    let mut builder = MockChain::builder();
    builder.add_output_note(OutputNote::Full(input_note.clone()));
    let mock_chain = builder.build()?;

    let tx_context = mock_chain.build_tx_context(account, &[input_note.id()], &[])?.build()?;
    let ref_block_num = tx_context.tx_inputs().block_header().block_num().as_u32();
    let final_nonce = tx_context.account().nonce().as_int() as u32 + 1;
    let input_notes = tx_context.input_notes().clone();
    let output_notes = OutputNotes::new(vec![OutputNote::Partial(output_note.into())])?;

    let error = tx_context.execute().await.unwrap_err();

    assert_matches!(error, TransactionExecutorError::Unauthorized(tx_summary) => {
        assert!(tx_summary.account_delta().vault().is_empty());
        assert!(tx_summary.account_delta().storage().is_empty());
        assert_eq!(tx_summary.account_delta().nonce_delta().as_int(), 1);
        assert_eq!(tx_summary.input_notes(), &input_notes);
        assert_eq!(tx_summary.output_notes(), &output_notes);
        assert_eq!(tx_summary.salt(), Word::from(
          [0, 0, ref_block_num, final_nonce]
        ));
    });

    Ok(())
}

/// Tests that a transaction consuming and creating one note with basic authentication correctly
/// signs the transaction summary.
#[tokio::test]
async fn tx_summary_commitment_is_signed_by_falcon_auth() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account(Auth::BasicAuth)?;
    let mut rng = RpoRandomCoin::new(Word::empty());
    let p2id_note = create_p2id_note(
        account.id(),
        account.id(),
        vec![],
        NoteType::Private,
        Felt::ZERO,
        &mut rng,
    )?;
    let spawn_note = builder.add_spawn_note([&p2id_note])?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account.id(), &[spawn_note.id()], &[])?
        .build()?
        .execute()
        .await?;

    let summary = TransactionSummary::new(
        tx.account_delta().clone(),
        tx.input_notes().clone(),
        tx.output_notes().clone(),
        Word::from([
            0,
            0,
            tx.block_header().block_num().as_u32(),
            tx.final_account().nonce().as_int() as u32,
        ]),
    );
    let summary_commitment = summary.to_commitment();

    let account_interface = AccountInterface::from(&account);
    let pub_key = match account_interface.auth().first().unwrap() {
        AuthScheme::RpoFalcon512 { pub_key } => pub_key,
        AuthScheme::NoAuth => panic!("Expected RpoFalcon512 auth scheme, got NoAuth"),
        AuthScheme::RpoFalcon512Multisig { .. } => {
            panic!("Expected RpoFalcon512 auth scheme, got RpoFalcon512Multisig")
        },
        AuthScheme::Unknown => panic!("Expected RpoFalcon512 auth scheme, got Unknown"),
        AuthScheme::EcdsaK256Keccak { .. } => {
            panic!("Expected RpoFalcon512 auth scheme, got EcdsaK256Keccak")
        },
        AuthScheme::EcdsaK256KeccakMultisig { .. } => {
            panic!("Expected RpoFalcon512 auth scheme, got EcdsaK256KeccakMultisig")
        },
    };

    // This is in an internal detail of the tx executor host, but this is the easiest way to check
    // for the presence of the signature in the advice map.
    let signature_key = Hasher::merge(&[Word::from(*pub_key), summary_commitment]);

    // The summary commitment should have been signed as part of transaction execution and inserted
    // into the advice map.
    tx.advice_witness().map.get(&signature_key).unwrap();

    Ok(())
}

/// Tests that a transaction consuming and creating one note with EcdsaK256Keccak authentication
/// correctly signs the transaction summary.
#[tokio::test]
async fn tx_summary_commitment_is_signed_by_ecdsa_auth() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account(Auth::EcdsaK256KeccakAuth)?;
    let mut rng = RpoRandomCoin::new(Word::empty());
    let p2id_note = create_p2id_note(
        account.id(),
        account.id(),
        vec![],
        NoteType::Private,
        Felt::ZERO,
        &mut rng,
    )?;
    let spawn_note = builder.add_spawn_note([&p2id_note])?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account.id(), &[spawn_note.id()], &[])?
        .build()?
        .execute()
        .await?;

    let summary = TransactionSummary::new(
        tx.account_delta().clone(),
        tx.input_notes().clone(),
        tx.output_notes().clone(),
        Word::from([
            0,
            0,
            tx.block_header().block_num().as_u32(),
            tx.final_account().nonce().as_int() as u32,
        ]),
    );
    let summary_commitment = summary.to_commitment();

    let account_interface = AccountInterface::from(&account);
    let pub_key = match account_interface.auth().first().unwrap() {
        AuthScheme::EcdsaK256Keccak { pub_key } => pub_key,
        AuthScheme::EcdsaK256KeccakMultisig { .. } => {
            panic!("Expected EcdsaK256Keccak auth scheme, got EcdsaK256KeccakMultisig")
        },
        AuthScheme::NoAuth => panic!("Expected EcdsaK256Keccak auth scheme, got NoAuth"),
        AuthScheme::RpoFalcon512Multisig { .. } => {
            panic!("Expected EcdsaK256Keccak auth scheme, got RpoFalcon512Multisig")
        },
        AuthScheme::Unknown => panic!("Expected EcdsaK256Keccak auth scheme, got Unknown"),
        AuthScheme::RpoFalcon512 { .. } => {
            panic!("Expected EcdsaK256Keccak auth scheme, got RpoFalcon512")
        },
    };

    // This is in an internal detail of the tx executor host, but this is the easiest way to check
    // for the presence of the signature in the advice map.
    let signature_key = Hasher::merge(&[Word::from(*pub_key), summary_commitment]);

    // The summary commitment should have been signed as part of transaction execution and inserted
    // into the advice map.
    tx.advice_witness().map.get(&signature_key).unwrap();

    Ok(())
}

/// Tests that execute_tx_view_script returns the expected stack outputs.
#[tokio::test]
async fn execute_tx_view_script() -> anyhow::Result<()> {
    let test_module_source = "
        export.foo
            push.3.4
            add
            swapw dropw
        end
    ";

    let source = NamedSource::new("test::module_1", test_module_source);
    let source_manager = Arc::new(DefaultSourceManager::default());
    let assembler = TransactionKernel::assembler_with_source_manager(source_manager.clone());

    let library = assembler.assemble_library([source]).unwrap();

    let source = "
    use.test::module_1
    use.std::sys

    begin
        push.1.2
        call.module_1::foo
        exec.sys::truncate_stack
    end
    ";

    let tx_script = ScriptBuilder::new(false)
        .with_statically_linked_library(&library)?
        .compile_tx_script(source)?;
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .with_source_manager(source_manager.clone())
        .tx_script(tx_script.clone())
        .build()?;
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let advice_inputs = tx_context.tx_args().advice_inputs().clone();

    let executor = TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context)
        .with_source_manager(source_manager);

    let stack_outputs = executor
        .execute_tx_view_script(account_id, block_ref, tx_script, advice_inputs)
        .await?;

    assert_eq!(stack_outputs[..3], [Felt::new(7), Felt::new(2), ONE]);

    Ok(())
}

// TEST TRANSACTION SCRIPT
// ================================================================================================

/// Tests transaction script inputs.
#[tokio::test]
async fn test_tx_script_inputs() -> anyhow::Result<()> {
    let tx_script_input_key = Word::from([9999, 8888, 9999, 8888u32]);
    let tx_script_input_value = Word::from([9, 8, 7, 6u32]);
    let tx_script_src = format!(
        "
        use.miden::account

        begin
            # push the tx script input key onto the stack
            push.{tx_script_input_key}

            # load the tx script input value from the map and read it onto the stack
            adv.push_mapval adv_loadw

            # assert that the value is correct
            push.{tx_script_input_value} assert_eqw
        end
        "
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(tx_script_src)?;

    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script)
        .extend_advice_map([(tx_script_input_key, tx_script_input_value.to_vec())])
        .build()?;

    tx_context.execute().await.context("failed to execute transaction")?;

    Ok(())
}

/// Tests transaction script arguments.
#[tokio::test]
async fn test_tx_script_args() -> anyhow::Result<()> {
    let tx_script_args = Word::from([1, 2, 3, 4u32]);

    let tx_script_src = r#"
        use.miden::account

        begin
            # => [TX_SCRIPT_ARGS]
            # `TX_SCRIPT_ARGS` value is a user provided word, which could be used during the
            # transaction execution. In this example it is a `[1, 2, 3, 4]` word.

            # assert the correctness of the argument
            dupw push.1.2.3.4 assert_eqw.err="provided transaction arguments don't match the expected ones"
            # => [TX_SCRIPT_ARGS]

            # since we provided an advice map entry with the transaction script arguments as a key,
            # we can obtain the value of this entry
            adv.push_mapval adv_push.4
            # => [[map_entry_values], TX_SCRIPT_ARGS]

            # assert the correctness of the map entry values
            push.5.6.7.8 assert_eqw.err="obtained advice map value doesn't match the expected one"
        end"#;

    let tx_script = ScriptBuilder::default()
        .compile_tx_script(tx_script_src)
        .context("failed to compile transaction script")?;

    // extend the advice map with the entry that is accessed using the provided transaction script
    // argument
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script)
        .extend_advice_map([(
            tx_script_args,
            vec![Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)],
        )])
        .tx_script_args(tx_script_args)
        .build()?;

    tx_context.execute().await?;

    Ok(())
}

// Tests that advice map from the account code and transaction script gets correctly passed as
// part of the transaction advice inputs
#[tokio::test]
async fn inputs_created_correctly() -> anyhow::Result<()> {
    let account_code_script = r#"
            adv_map.A([6,7,8,9])=[10,11,12,13]

            export.assert_adv_map
                # test tx script advice map
                push.[1,2,3,4]
                adv.push_mapval adv_loadw
                push.[5,6,7,8]
                assert_eqw.err="script adv map not found"
            end
        "#;

    let component = AccountComponent::compile(
        account_code_script,
        TransactionKernel::assembler(),
        vec![StorageSlot::Value(Word::default())],
    )?
    .with_supports_all_types();

    let account_code = AccountCode::from_components(
        &[IncrNonceAuthComponent.into(), component.clone()],
        AccountType::RegularAccountUpdatableCode,
    )?;

    let script = format!(
        r#"
            use.miden::account

            adv_map.A([1,2,3,4])=[5,6,7,8]

            begin
                # test account code advice map
                push.[6,7,8,9]
                adv.push_mapval adv_loadw
                push.[10,11,12,13]
                assert_eqw.err="account code adv map not found"

                call.{assert_adv_map_proc_root}
            end
        "#,
        assert_adv_map_proc_root =
            component.library().get_procedure_root_by_name("$anon::assert_adv_map").unwrap()
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(script)?;

    assert!(tx_script.mast().advice_map().get(&Word::try_from([1u64, 2, 3, 4])?).is_some());
    assert!(
        account_code
            .mast()
            .advice_map()
            .get(&Word::try_from([6u64, 7, 8, 9])?)
            .is_some()
    );

    let account = Account::new_existing(
        ACCOUNT_ID_PRIVATE_SENDER.try_into()?,
        AssetVault::mock(),
        AccountStorage::mock(),
        account_code,
        Felt::new(1u64),
    );
    let tx_context = crate::TransactionContextBuilder::new(account).tx_script(tx_script).build()?;
    _ = tx_context.execute().await?;

    Ok(())
}

/// Test that reexecuting a transaction with no authenticator and the tx inputs from a first
/// successful execution is possible. This ensures that the signature generated in the first
/// execution is present during re-execution.
#[tokio::test]
async fn tx_can_be_reexecuted() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    // Use basic auth so the tx requires a signature for successful execution.
    let account = builder.add_existing_mock_account(Auth::BasicAuth)?;
    let note = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[FungibleAsset::mock(3)],
        NoteType::Public,
    )?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account.id(), &[note.id()], &[])?
        .build()?
        .execute()
        .await?;

    let _reexecuted_tx = chain
        .build_tx_context(account.id(), &[note.id()], &[])?
        .authenticator(None)
        .tx_inputs(tx.tx_inputs().clone())
        .build()?
        .execute()
        .await?;

    Ok(())
}
