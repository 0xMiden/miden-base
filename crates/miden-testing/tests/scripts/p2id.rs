use miden_lib::{
    errors::note_script_errors::ERR_P2ID_TARGET_ACCT_MISMATCH, note::create_p2id_note,
    transaction::TransactionKernel,
};
use miden_objects::{
    Felt, Word,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::NoteType,
    testing::account_id::{
        ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2, ACCOUNT_ID_SENDER,
    },
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain};
use miden_tx::utils::word_to_masm_push_string;

use crate::{assert_transaction_executor_error, prove_and_verify_transaction};

/// We test the Pay to script with 2 assets to test the loop inside the script.
/// So we create a note containing two assets that can only be consumed by the target account.
#[test]
fn p2id_script_multiple_assets() -> anyhow::Result<()> {
    // Create assets
    let fungible_asset_1: Asset = FungibleAsset::mock(123);
    let fungible_asset_2: Asset =
        FungibleAsset::new(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into()?, 456)?.into();

    let mut builder = MockChain::builder();

    // Create accounts
    let sender_account = builder.create_new_wallet(Auth::BasicAuth)?;
    let target_account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let malicious_account = builder.add_existing_wallet(Auth::BasicAuth)?;

    // Create the note
    let note = builder.add_p2id_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset_1, fungible_asset_2],
        NoteType::Public,
    )?;

    let mock_chain = builder.build()?;

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------
    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(target_account.id(), &[note.id()], &[])?
        .build()?
        .execute()?;

    // vault delta
    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset_1, fungible_asset_2]).unwrap(),
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(
        executed_transaction.final_account().commitment(),
        target_account_after.commitment()
    );

    // CONSTRUCT AND EXECUTE TX (Failure)
    // --------------------------------------------------------------------------------------------
    // A "malicious" account tries to consume the note, we expect an error (not the correct target)

    // Execute the transaction and get the result
    let executed_transaction_2 = mock_chain
        .build_tx_context(malicious_account.id(), &[], &[note])?
        .build()?
        .execute();

    // Check that we got the expected result - TransactionExecutorError
    assert_transaction_executor_error!(executed_transaction_2, ERR_P2ID_TARGET_ACCT_MISMATCH);
    Ok(())
}

/// Consumes an existing note with a new account
#[test]
fn prove_consume_note_with_new_account() -> anyhow::Result<()> {
    // Create assets
    let fungible_asset: Asset = FungibleAsset::mock(123);

    let mut builder = MockChain::builder();

    // Create accounts
    let sender_account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let target_account = builder.create_new_wallet(Auth::BasicAuth)?;

    // Create the note
    let note = builder.add_p2id_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
    )?;

    let mock_chain = builder.build()?;

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------

    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(target_account.clone(), &[note.id()], &[])?
        .build()?
        .execute()?;

    // Apply delta to the target account to verify it is no longer new
    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset]).unwrap(),
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(1),
    );

    assert_eq!(
        executed_transaction.final_account().commitment(),
        target_account_after.commitment()
    );
    prove_and_verify_transaction(executed_transaction)?;
    Ok(())
}

/// Consumes two existing notes (with an asset from a faucet for a combined total of 123 tokens)
/// with a basic account
#[test]
fn prove_consume_multiple_notes() -> anyhow::Result<()> {
    let fungible_asset_1: Asset = FungibleAsset::mock(100);
    let fungible_asset_2: Asset = FungibleAsset::mock(23);

    let mut builder = MockChain::builder();
    let mut account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let note_1 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[fungible_asset_1],
        NoteType::Private,
    )?;
    let note_2 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[fungible_asset_2],
        NoteType::Private,
    )?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[note_1.id(), note_2.id()], &[])?
        .build()?;

    let executed_transaction = tx_context.execute()?;

    account.apply_delta(executed_transaction.account_delta())?;
    let resulting_asset = account.vault().assets().next().unwrap();
    if let Asset::Fungible(asset) = resulting_asset {
        assert_eq!(asset.amount(), 123u64);
    } else {
        panic!("Resulting asset should be fungible");
    }

    Ok(prove_and_verify_transaction(executed_transaction)?)
}

/// Consumes two existing notes and creates two other notes in the same transaction
#[test]
fn test_create_consume_multiple_notes() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let mut account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, [FungibleAsset::mock(20)])?;

    let input_note_faucet_id = ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET.try_into()?;
    let input_note_asset_1: Asset = FungibleAsset::new(input_note_faucet_id, 11)?.into();

    let input_note_asset_2: Asset = FungibleAsset::new(input_note_faucet_id, 100)?.into();

    let input_note_1 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[input_note_asset_1],
        NoteType::Private,
    )?;

    let input_note_2 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into()?,
        account.id(),
        &[input_note_asset_2],
        NoteType::Private,
    )?;

    let mock_chain = builder.build()?;

    let output_note_1 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into()?,
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    let output_note_2 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(5)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([4, 3, 2, 1u32])),
    )?;

    let tx_script_src = &format!(
        "
            use.miden::tx
            begin
                push.{recipient_1}
                push.{note_execution_hint_1}
                push.{note_type_1}
                push.0              # aux
                push.{tag_1}
                call.tx::create_note

                push.{asset_1}
                call.::miden::contracts::wallets::basic::move_asset_to_note
                dropw dropw dropw dropw

                push.{recipient_2}
                push.{note_execution_hint_2}
                push.{note_type_2}
                push.0              # aux
                push.{tag_2}
                call.tx::create_note

                push.{asset_2}
                call.::miden::contracts::wallets::basic::move_asset_to_note
                dropw dropw dropw dropw
            end
            ",
        recipient_1 = word_to_masm_push_string(&output_note_1.recipient().digest()),
        note_type_1 = NoteType::Public as u8,
        tag_1 = Felt::from(output_note_1.metadata().tag()),
        asset_1 = word_to_masm_push_string(&FungibleAsset::mock(10).into()),
        note_execution_hint_1 = Felt::from(output_note_1.metadata().execution_hint()),
        recipient_2 = word_to_masm_push_string(&output_note_2.recipient().digest()),
        note_type_2 = NoteType::Public as u8,
        tag_2 = Felt::from(output_note_2.metadata().tag()),
        asset_2 = word_to_masm_push_string(&FungibleAsset::mock(5).into()),
        note_execution_hint_2 = Felt::from(output_note_2.metadata().execution_hint())
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[input_note_1.id(), input_note_2.id()], &[])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(output_note_1),
            OutputNote::Full(output_note_2),
        ])
        .tx_script(tx_script)
        .build()?;

    let executed_transaction = tx_context.execute()?;

    assert_eq!(executed_transaction.output_notes().num_notes(), 2);

    account.apply_delta(executed_transaction.account_delta())?;

    assert_eq!(account.vault().get_balance(input_note_faucet_id)?, 111);
    assert_eq!(account.vault().get_balance(FungibleAsset::mock_issuer())?, 5);
    Ok(())
}
