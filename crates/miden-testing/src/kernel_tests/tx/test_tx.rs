use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use anyhow::Context;
use assert_matches::assert_matches;
use miden_lib::AuthScheme;
use miden_lib::account::interface::AccountInterface;
use miden_lib::account::wallets::BasicWallet;
use miden_lib::errors::tx_kernel_errors::{
    ERR_NON_FUNGIBLE_ASSET_ALREADY_EXISTS,
    ERR_TX_NUMBER_OF_OUTPUT_NOTES_EXCEEDS_LIMIT,
};
use miden_lib::note::create_p2id_note;
use miden_lib::testing::account_component::IncrNonceAuthComponent;
use miden_lib::testing::mock_account::MockAccountExt;
use miden_lib::transaction::memory::{
    NOTE_MEM_SIZE,
    NUM_OUTPUT_NOTES_PTR,
    OUTPUT_NOTE_ASSETS_OFFSET,
    OUTPUT_NOTE_METADATA_OFFSET,
    OUTPUT_NOTE_RECIPIENT_OFFSET,
    OUTPUT_NOTE_SECTION_OFFSET,
};
use miden_lib::transaction::{TransactionEvent, TransactionKernel};
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountCode,
    AccountComponent,
    AccountId,
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
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PRIVATE_SENDER,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    ACCOUNT_ID_SENDER,
};
use miden_objects::testing::constants::{
    FUNGIBLE_ASSET_AMOUNT,
    NON_FUNGIBLE_ASSET_DATA,
    NON_FUNGIBLE_ASSET_DATA_2,
};
use miden_objects::testing::note::DEFAULT_NOTE_CODE;
use miden_objects::transaction::{
    InputNotes,
    OutputNote,
    OutputNotes,
    TransactionArgs,
    TransactionSummary,
};
use miden_objects::{FieldElement, Hasher, Word};
use miden_processor::crypto::RpoRandomCoin;
use miden_tx::auth::UnreachableAuth;
use miden_tx::{TransactionExecutor, TransactionExecutorError};

use super::{Felt, ONE, ZERO};
use crate::kernel_tests::tx::ProcessMemoryExt;
use crate::utils::{create_p2any_note, create_spawn_note};
use crate::{Auth, MockChain, TransactionContextBuilder, assert_execution_error};

/// Tests that executing a transaction with a foreign account whose inputs are stale fails.
#[test]
fn transaction_with_stale_foreign_account_inputs_fails() -> anyhow::Result<()> {
    // Create a chain with an account
    let mut builder = MockChain::builder();
    let native_account = builder.add_existing_wallet(Auth::IncrNonce)?;
    let foreign_account = builder.add_existing_wallet(Auth::IncrNonce)?;
    let new_account = builder.create_new_wallet(Auth::IncrNonce)?;

    let mut mock_chain = builder.build()?;

    // Retrieve inputs which will become stale
    let inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    // Create a new unrelated account to modify the account tree.
    let tx = mock_chain
        .build_tx_context(new_account, &[], &[])?
        .build()?
        .execute_blocking()?;
    mock_chain.add_pending_executed_transaction(&tx)?;
    mock_chain.prove_next_block()?;

    // Attempt to execute with older foreign account inputs. The AccountWitness in the foreign
    // account's inputs have become stale and so this should fail.
    let transaction = mock_chain
        .build_tx_context(native_account.id(), &[], &[])?
        .foreign_accounts(vec![inputs])
        .build()?
        .execute_blocking();

    assert_matches::assert_matches!(
        transaction,
        Err(TransactionExecutorError::ForeignAccountNotAnchoredInReference(_))
    );
    Ok(())
}

/// Tests that consuming a note created in a block that is newer than the reference block of the
/// transaction fails.
#[tokio::test]
async fn consuming_note_created_in_future_block_fails() -> anyhow::Result<()> {
    // Create a chain with an account
    let mut builder = MockChain::builder();
    let asset = FungibleAsset::mock(400);
    let account1 = builder.add_existing_wallet_with_assets(Auth::BasicAuth, [asset])?;
    let account2 = builder.add_existing_wallet_with_assets(Auth::BasicAuth, [asset])?;
    let output_note = create_p2any_note(account1.id(), [asset]);
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

#[test]
fn test_create_note() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;
    let account_id = tx_context.account().id();

    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(account_id);

    let code = format!(
        "
        use.miden::tx

        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{note_execution_hint}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note

            # truncate the stack
            swapdw dropw dropw
        end
        ",
        recipient = recipient,
        PUBLIC_NOTE = NoteType::Public as u8,
        note_execution_hint = Felt::from(NoteExecutionHint::after_block(23.into()).unwrap()),
        tag = tag,
    );

    let process = &tx_context.execute_code(&code)?;

    assert_eq!(
        process.get_kernel_mem_word(NUM_OUTPUT_NOTES_PTR),
        Word::from([1, 0, 0, 0u32]),
        "number of output notes must increment by 1",
    );

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_RECIPIENT_OFFSET),
        recipient,
        "recipient must be stored at the correct memory location",
    );

    let expected_note_metadata: Word = NoteMetadata::new(
        account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::after_block(23.into())?,
        Felt::new(27),
    )?
    .into();

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET),
        expected_note_metadata,
        "metadata must be stored at the correct memory location",
    );

    assert_eq!(
        process.stack.get(0),
        ZERO,
        "top item on the stack is the index of the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_with_invalid_tag() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let invalid_tag = Felt::new((NoteType::Public as u64) << 62);
    let valid_tag: Felt = NoteTag::for_local_use_case(0, 0).unwrap().into();

    // Test invalid tag
    assert!(tx_context.execute_code(&note_creation_script(invalid_tag)).is_err());

    // Test valid tag
    assert!(tx_context.execute_code(&note_creation_script(valid_tag)).is_ok());

    Ok(())
}

fn note_creation_script(tag: Felt) -> String {
    format!(
        "
            use.miden::tx
            use.$kernel::prologue

            begin
                exec.prologue::prepare_transaction

                push.{recipient}
                push.{execution_hint_always}
                push.{PUBLIC_NOTE}
                push.{aux}
                push.{tag}

                call.tx::create_note

                # clean the stack
                dropw dropw
            end
            ",
        recipient = Word::from([0, 1, 2, 3u32]),
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        PUBLIC_NOTE = NoteType::Public as u8,
        aux = Felt::ZERO,
    )
}

#[test]
fn test_create_note_too_many_notes() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let code = format!(
        "
        use.miden::tx
        use.$kernel::constants
        use.$kernel::memory
        use.$kernel::prologue

        begin
            exec.constants::get_max_num_output_notes
            exec.memory::set_num_output_notes
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{execution_hint_always}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
        end
        ",
        tag = NoteTag::for_local_use_case(1234, 5678).unwrap(),
        recipient = Word::from([0, 1, 2, 3u32]),
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        PUBLIC_NOTE = NoteType::Public as u8,
        aux = Felt::ZERO,
    );

    let process = tx_context.execute_code(&code);

    assert_execution_error!(process, ERR_TX_NUMBER_OF_OUTPUT_NOTES_EXCEEDS_LIMIT);
    Ok(())
}

#[test]
fn test_get_output_notes_commitment() -> anyhow::Result<()> {
    let tx_context = {
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, Auth::IncrNonce);

        let output_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into()?, [FungibleAsset::mock(100)]);

        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_PRIVATE_SENDER.try_into()?, [FungibleAsset::mock(100)]);

        let input_note_2 =
            create_p2any_note(ACCOUNT_ID_PRIVATE_SENDER.try_into()?, [FungibleAsset::mock(200)]);

        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2])
            .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
            .build()?
    };

    // extract input note data
    let input_note_1 = tx_context.tx_inputs().input_notes().get_note(0).note();
    let input_asset_1 = **input_note_1
        .assets()
        .iter()
        .take(1)
        .collect::<Vec<_>>()
        .first()
        .context("getting first expected input asset")?;
    let input_note_2 = tx_context.tx_inputs().input_notes().get_note(1).note();
    let input_asset_2 = **input_note_2
        .assets()
        .iter()
        .take(1)
        .collect::<Vec<_>>()
        .first()
        .context("getting second expected input asset")?;

    // Choose random accounts as the target for the note tag.
    let network_account = AccountId::try_from(ACCOUNT_ID_NETWORK_NON_FUNGIBLE_FAUCET)?;
    let local_account = AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET)?;

    // create output note 1
    let output_serial_no_1 = Word::from([8u32; 4]);
    let output_tag_1 = NoteTag::from_account_id(network_account);
    let assets = NoteAssets::new(vec![input_asset_1])?;
    let metadata = NoteMetadata::new(
        tx_context.tx_inputs().account().id(),
        NoteType::Public,
        output_tag_1,
        NoteExecutionHint::Always,
        ZERO,
    )?;
    let inputs = NoteInputs::new(vec![])?;
    let recipient = NoteRecipient::new(output_serial_no_1, input_note_1.script().clone(), inputs);
    let output_note_1 = Note::new(assets, metadata, recipient);

    // create output note 2
    let output_serial_no_2 = Word::from([11u32; 4]);
    let output_tag_2 = NoteTag::from_account_id(local_account);
    let assets = NoteAssets::new(vec![input_asset_2])?;
    let metadata = NoteMetadata::new(
        tx_context.tx_inputs().account().id(),
        NoteType::Public,
        output_tag_2,
        NoteExecutionHint::after_block(123.into())?,
        ZERO,
    )?;
    let inputs = NoteInputs::new(vec![])?;
    let recipient = NoteRecipient::new(output_serial_no_2, input_note_2.script().clone(), inputs);
    let output_note_2 = Note::new(assets, metadata, recipient);

    // compute expected output notes commitment
    let expected_output_notes_commitment = OutputNotes::new(vec![
        OutputNote::Full(output_note_1.clone()),
        OutputNote::Full(output_note_2.clone()),
    ])?
    .commitment();

    let code = format!(
        "
        use.std::sys

        use.miden::tx

        use.$kernel::prologue
        use.mock::account

        begin
            # => [BH, acct_id, IAH, NC]
            exec.prologue::prepare_transaction
            # => []

            # create output note 1
            push.{recipient_1}
            push.{NOTE_EXECUTION_HINT_1}
            push.{PUBLIC_NOTE}
            push.{aux_1}
            push.{tag_1}
            call.tx::create_note
            # => [note_idx]

            push.{asset_1}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw drop
            # => []

            # create output note 2
            push.{recipient_2}
            push.{NOTE_EXECUTION_HINT_2}
            push.{PUBLIC_NOTE}
            push.{aux_2}
            push.{tag_2}
            call.tx::create_note
            # => [note_idx]

            push.{asset_2}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw drop
            # => []

            # compute the output notes commitment
            exec.tx::get_output_notes_commitment
            # => [OUTPUT_NOTES_COMMITMENT]

            # truncate the stack
            exec.sys::truncate_stack
            # => [OUTPUT_NOTES_COMMITMENT]
        end
        ",
        PUBLIC_NOTE = NoteType::Public as u8,
        NOTE_EXECUTION_HINT_1 = Felt::from(output_note_1.metadata().execution_hint()),
        recipient_1 = output_note_1.recipient().digest(),
        tag_1 = output_note_1.metadata().tag(),
        aux_1 = output_note_1.metadata().aux(),
        asset_1 = Word::from(
            **output_note_1.assets().iter().take(1).collect::<Vec<_>>().first().unwrap()
        ),
        recipient_2 = output_note_2.recipient().digest(),
        NOTE_EXECUTION_HINT_2 = Felt::from(output_note_2.metadata().execution_hint()),
        tag_2 = output_note_2.metadata().tag(),
        aux_2 = output_note_2.metadata().aux(),
        asset_2 = Word::from(
            **output_note_2.assets().iter().take(1).collect::<Vec<_>>().first().unwrap()
        ),
    );

    let process = &tx_context.execute_code(&code)?;

    assert_eq!(
        process.get_kernel_mem_word(NUM_OUTPUT_NOTES_PTR),
        Word::from([2u32, 0, 0, 0]),
        "The test creates two notes",
    );
    assert_eq!(
        NoteMetadata::try_from(
            process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET)
        )
        .unwrap(),
        *output_note_1.metadata(),
        "Validate the output note 1 metadata",
    );
    assert_eq!(
        NoteMetadata::try_from(process.get_kernel_mem_word(
            OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_METADATA_OFFSET + NOTE_MEM_SIZE
        ))
        .unwrap(),
        *output_note_2.metadata(),
        "Validate the output note 1 metadata",
    );

    assert_eq!(process.stack.get_word(0), expected_output_notes_commitment);
    Ok(())
}

#[test]
fn test_create_note_and_add_asset() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_id);
    let asset = Word::from(FungibleAsset::new(faucet_id, 10)?);

    let code = format!(
        "
        use.miden::tx

        use.$kernel::prologue
        use.mock::account

        begin
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{NOTE_EXECUTION_HINT}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx]

            push.{asset}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw
            # => [note_idx]

            # truncate the stack
            swapdw dropw dropw
        end
        ",
        recipient = recipient,
        PUBLIC_NOTE = NoteType::Public as u8,
        NOTE_EXECUTION_HINT = Felt::from(NoteExecutionHint::always()),
        tag = tag,
        asset = asset,
    );

    let process = &tx_context.execute_code(&code)?;

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET),
        asset,
        "asset must be stored at the correct memory location",
    );

    assert_eq!(
        process.stack.get(0),
        ZERO,
        "top item on the stack is the index to the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_and_add_multiple_assets() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let faucet = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let faucet_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2)?;

    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_2);

    let asset = Word::from(FungibleAsset::new(faucet, 10)?);
    let asset_2 = Word::from(FungibleAsset::new(faucet_2, 20)?);
    let asset_3 = Word::from(FungibleAsset::new(faucet_2, 30)?);
    let asset_2_and_3 = Word::from(FungibleAsset::new(faucet_2, 50)?);

    let non_fungible_asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);
    let non_fungible_asset_encoded = Word::from(non_fungible_asset);

    let code = format!(
        "
        use.miden::tx

        use.$kernel::prologue
        use.mock::account

        begin
            exec.prologue::prepare_transaction

            push.{recipient}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx]

            push.{asset}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{asset_2}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{asset_3}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            push.{nft}
            call.tx::add_asset_to_note dropw
            # => [note_idx]

            # truncate the stack
            swapdw dropw drop drop drop
        end
        ",
        recipient = recipient,
        PUBLIC_NOTE = NoteType::Public as u8,
        tag = tag,
        asset = asset,
        asset_2 = asset_2,
        asset_3 = asset_3,
        nft = non_fungible_asset_encoded,
    );

    let process = &tx_context.execute_code(&code)?;

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET),
        asset,
        "asset must be stored at the correct memory location",
    );

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET + 4),
        asset_2_and_3,
        "asset_2 and asset_3 must be stored at the same correct memory location",
    );

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_ASSETS_OFFSET + 8),
        non_fungible_asset_encoded,
        "non_fungible_asset must be stored at the correct memory location",
    );

    assert_eq!(
        process.stack.get(0),
        ZERO,
        "top item on the stack is the index to the output note"
    );
    Ok(())
}

#[test]
fn test_create_note_and_add_same_nft_twice() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let recipient = Word::from([0, 1, 2, 3u32]);
    let tag = NoteTag::for_public_use_case(999, 777, NoteExecutionMode::Local).unwrap();
    let non_fungible_asset = NonFungibleAsset::mock(&[1, 2, 3]);
    let encoded = Word::from(non_fungible_asset);

    let code = format!(
        "
        use.$kernel::prologue
        use.mock::account
        use.miden::tx

        begin
            exec.prologue::prepare_transaction
            # => []

            padw padw
            push.{recipient}
            push.{execution_hint_always}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx, pad(15)]

            push.{nft}
            call.tx::add_asset_to_note
            # => [NFT, note_idx, pad(15)]
            dropw

            push.{nft}
            call.tx::add_asset_to_note
            # => [NFT, note_idx, pad(15)]

            repeat.5 dropw end
        end
        ",
        recipient = recipient,
        PUBLIC_NOTE = NoteType::Public as u8,
        execution_hint_always = Felt::from(NoteExecutionHint::always()),
        aux = Felt::new(0),
        tag = tag,
        nft = encoded,
    );

    let process = tx_context.execute_code(&code);

    assert_execution_error!(process, ERR_NON_FUNGIBLE_ASSET_ALREADY_EXISTS);
    Ok(())
}

/// Tests that creating a note with a fungible asset with amount zero works.
#[test]
fn creating_note_with_fungible_asset_amount_zero_works() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account(Auth::IncrNonce)?;
    let output_note = builder.add_p2id_note(
        account.id(),
        account.id(),
        &[FungibleAsset::mock(0)],
        NoteType::Private,
    )?;
    let input_note = builder.add_spawn_note([&output_note])?;
    let chain = builder.build()?;

    chain
        .build_tx_context(account, &[input_note.id()], &[])?
        .build()?
        .execute_blocking()?;

    Ok(())
}

#[test]
fn test_build_recipient_hash() -> anyhow::Result<()> {
    let tx_context = {
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, Auth::IncrNonce);

        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), [FungibleAsset::mock(100)]);
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1])
            .build()?
    };
    let input_note_1 = tx_context.tx_inputs().input_notes().get_note(0).note();

    // create output note
    let output_serial_no = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::for_public_use_case(42, 42, NoteExecutionMode::Network).unwrap();
    let single_input = 2;
    let inputs = NoteInputs::new(vec![Felt::new(single_input)]).unwrap();
    let input_commitment = inputs.commitment();

    let recipient = NoteRecipient::new(output_serial_no, input_note_1.script().clone(), inputs);
    let code = format!(
        "
        use.miden::tx
        use.miden::note
        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # pad the stack before call
            padw

            # input
            push.{input_commitment}
            # SCRIPT_ROOT
            push.{script_root}
            # SERIAL_NUM
            push.{output_serial_no}
            # => [SERIAL_NUM, SCRIPT_ROOT, INPUT_COMMITMENT, pad(4)]

            exec.note::build_recipient_hash
            # => [RECIPIENT, pad(12)]

            push.{execution_hint}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}
            # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(12)]

            call.tx::create_note
            # => [note_idx, pad(19)]

            # clean the stack
            dropw dropw dropw dropw dropw
        end
        ",
        script_root = input_note_1.script().clone().root(),
        output_serial_no = output_serial_no,
        PUBLIC_NOTE = NoteType::Public as u8,
        tag = tag,
        execution_hint = Felt::from(NoteExecutionHint::after_block(2.into()).unwrap()),
        aux = aux,
    );

    let process = &tx_context.execute_code(&code)?;

    assert_eq!(
        process.get_kernel_mem_word(NUM_OUTPUT_NOTES_PTR),
        Word::from([1, 0, 0, 0u32]),
        "number of output notes must increment by 1",
    );

    let recipient_digest = recipient.clone().digest();

    assert_eq!(
        process.get_kernel_mem_word(OUTPUT_NOTE_SECTION_OFFSET + OUTPUT_NOTE_RECIPIENT_OFFSET),
        recipient_digest,
        "recipient hash not correct",
    );
    Ok(())
}

// BLOCK TESTS
// ================================================================================================

#[test]
fn test_block_procedures() -> anyhow::Result<()> {
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

    let process = &tx_context.execute_code(code)?;

    assert_eq!(
        process.stack.get_word(0),
        tx_context.tx_inputs().block_header().commitment(),
        "top word on the stack should be equal to the block header commitment"
    );

    assert_eq!(
        process.stack.get(4).as_int(),
        tx_context.tx_inputs().block_header().timestamp() as u64,
        "fifth element on the stack should be equal to the timestamp of the last block creation"
    );

    assert_eq!(
        process.stack.get(5).as_int(),
        tx_context.tx_inputs().block_header().block_num().as_u64(),
        "sixth element on the stack should be equal to the block number"
    );
    Ok(())
}

#[test]
fn executed_transaction_output_notes() -> anyhow::Result<()> {
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
    let inputs_2 = NoteInputs::new(vec![ONE])?;
    let metadata_2 =
        NoteMetadata::new(account_id, note_type2, tag2, NoteExecutionHint::none(), aux2)?;
    let vault_2 = NoteAssets::new(vec![removed_asset_3, removed_asset_4])?;
    let recipient_2 = NoteRecipient::new(serial_num_2, note_script_2, inputs_2);
    let expected_output_note_2 = Note::new(vault_2, metadata_2, recipient_2);

    // Create the expected output note for Note 3 which is public
    let serial_num_3 = Word::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]);
    let note_script_3 = ScriptBuilder::default().compile_note_script(DEFAULT_NOTE_CODE)?;
    let inputs_3 = NoteInputs::new(vec![ONE, Felt::new(2)])?;
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
        use.miden::tx
        use.mock::account

        # Inputs:  [tag, aux, note_type, execution_hint, RECIPIENT]
        # Outputs: [note_idx]
        proc.create_note
            # pad the stack before the call to prevent accidental modification of the deeper stack
            # elements
            padw padw swapdw
            # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8)]

            call.tx::create_note
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

    let executed_transaction = tx_context.execute_blocking()?;

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

    // make sure that the number of note inputs remains the same
    let resulting_note_2_recipient =
        resulting_output_note_2.recipient().expect("output note 2 is not full");
    assert_eq!(
        resulting_note_2_recipient.inputs().num_values(),
        expected_output_note_2.inputs().num_values()
    );

    let resulting_note_3_recipient =
        resulting_output_note_3.recipient().expect("output note 3 is not full");
    assert_eq!(
        resulting_note_3_recipient.inputs().num_values(),
        expected_output_note_3.inputs().num_values()
    );

    Ok(())
}

/// Tests that a transaction consuming and creating one note can emit an abort event in its auth
/// component to result in a [`TransactionExecutorError::Unauthorized`] error.
#[test]
fn user_code_can_abort_transaction_with_summary() -> anyhow::Result<()> {
    let source_code = format!(
        "
      use.miden::auth
      use.miden::tx
      #! Inputs:  [AUTH_ARGS, pad(12)]
      #! Outputs: [pad(16)]
      export.auth__abort_tx
          dropw
          # => [pad(16)]

          push.0.0 exec.tx::get_block_number
          exec.::miden::account::incr_nonce
          # => [[final_nonce, block_num, 0, 0], pad(16)]
          # => [SALT, pad(16)]

          exec.auth::create_tx_summary
          # => [SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT]

          exec.auth::adv_insert_hqword

          exec.auth::hash_tx_summary
          # => [MESSAGE, pad(16)]

          emit.{abort_event}
      end
    ",
        abort_event = TransactionEvent::Unauthorized as u32
    );

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

    let mut mock_chain = MockChain::new();

    mock_chain.add_pending_note(OutputNote::Full(input_note.clone()));
    mock_chain.prove_next_block()?;

    let tx_context = mock_chain.build_tx_context(account, &[input_note.id()], &[])?.build()?;
    let ref_block_num = tx_context.tx_inputs().block_header().block_num().as_u32();
    let final_nonce = tx_context.account().nonce().as_int() as u32 + 1;
    let input_notes = tx_context.input_notes().clone();
    let output_notes = OutputNotes::new(vec![OutputNote::Partial(output_note.into())])?;

    let error = tx_context.execute_blocking().unwrap_err();

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
#[test]
fn tx_summary_commitment_is_signed_by_falcon_auth() -> anyhow::Result<()> {
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
        .execute_blocking()?;

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
            panic!("Expected RpoFalcon512 auth scheme, got Multisig")
        },
        AuthScheme::Unknown => panic!("Expected RpoFalcon512 auth scheme, got Unknown"),
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
        .execute_tx_view_script(account_id, block_ref, tx_script, advice_inputs, Vec::default())
        .await?;

    assert_eq!(stack_outputs[..3], [Felt::new(7), Felt::new(2), ONE]);

    Ok(())
}

// TEST TRANSACTION SCRIPT
// ================================================================================================

/// Tests transaction script inputs.
#[test]
fn test_tx_script_inputs() -> anyhow::Result<()> {
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

    tx_context.execute_blocking().context("failed to execute transaction")?;

    Ok(())
}

/// Tests transaction script arguments.
#[test]
fn test_tx_script_args() -> anyhow::Result<()> {
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

    tx_context.execute_blocking()?;

    Ok(())
}

// Tests that advice map from the account code and transaction script gets correctly passed as
// part of the transaction advice inputs
#[test]
fn inputs_created_correctly() -> anyhow::Result<()> {
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

    let account = Account::from_parts(
        ACCOUNT_ID_PRIVATE_SENDER.try_into()?,
        AssetVault::mock(),
        AccountStorage::mock(),
        account_code,
        Felt::new(1u64),
    );
    let tx_context = crate::TransactionContextBuilder::new(account).tx_script(tx_script).build()?;
    _ = tx_context.execute_blocking()?;

    Ok(())
}
