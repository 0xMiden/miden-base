use std::{fs, path::Path};

use anyhow::Context;
use miden_crypto::Word;
use miden_lib::{
    errors::note_script_errors::{
        ERR_P2IDH_RECLAIM_HEIGHT_NOT_REACHED, ERR_P2IDH_TIMELOCK_NOT_REACHED,
    },
    note::create_p2idh_note,
    transaction::TransactionKernel,
};
use miden_objects::{
    Felt, ONE,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteExecutionMode, NoteInputs, NoteMetadata,
        NoteRecipient, NoteScript, NoteTag, NoteType,
    },
    transaction::OutputNote,
};
use miden_testing::{Auth, MockChain};

use crate::assert_transaction_executor_error;

#[test]
fn hybrid_p2id_script_success() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let hybrid_p2id = create_p2idh_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        None,
        None,
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let output_note = OutputNote::Full(hybrid_p2id.clone());
    mock_chain.add_pending_note(output_note);
    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute()
        .unwrap();

    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset]).unwrap(),
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );
    assert_eq!(
        executed_transaction_1.final_account().commitment(),
        target_account_after.commitment()
    );

    Ok(())
}

#[test]
fn hybrid_p2id_script_success_1() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let code = fs::read_to_string(Path::new("../miden-lib/asm/note_scripts/P2IDH.masm")).unwrap();

    let serial_num = Word::default();
    let note_script = NoteScript::compile(code, assembler).unwrap();

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let reclaim_block_height = Felt::new(0); // if 0, means it is not reclaimable
    let timelock_block_height = Felt::new(0); // if 0 means it is not timelocked

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let note_inputs = NoteInputs::new(vec![
        target_account.id().suffix(),
        target_account.id().prefix().into(),
        reclaim_block_height,
        timelock_block_height,
    ])
    .unwrap();

    let recipient = NoteRecipient::new(serial_num, note_script, note_inputs.clone());
    let tag = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;

    let fungible_asset: Asset = FungibleAsset::mock(100);
    let vault = NoteAssets::new(vec![fungible_asset])?;
    let hybrid_p2id = Note::new(vault, metadata, recipient);

    let output_note = OutputNote::Full(hybrid_p2id.clone());
    mock_chain.add_pending_note(output_note);
    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute()
        .unwrap();

    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset]).unwrap(),
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );
    assert_eq!(
        executed_transaction_1.final_account().commitment(),
        target_account_after.commitment()
    );

    Ok(())
}

#[test]
fn hybrid_p2id_script_reclaim_test() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    // let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let code = fs::read_to_string(Path::new("../miden-lib/asm/note_scripts/P2IDH.masm")).unwrap();
    let serial_num = Word::default();
    let note_script = NoteScript::compile(code, assembler).unwrap();

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let reclaim_block_height = Felt::new(5); // if 0, means it is not reclaimable
    let timelock_block_height = Felt::new(0); // if 0 means it is not timelocked

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let note_inputs = NoteInputs::new(vec![
        target_account.id().suffix(),
        target_account.id().prefix().into(),
        reclaim_block_height,
        timelock_block_height,
    ])
    .unwrap();

    let recipient = NoteRecipient::new(serial_num, note_script, note_inputs.clone());
    let tag = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;

    let fungible_asset: Asset = FungibleAsset::mock(100);
    let vault = NoteAssets::new(vec![fungible_asset])?;
    let hybrid_p2id = Note::new(vault, metadata, recipient);

    let output_note = OutputNote::Full(hybrid_p2id.clone());
    mock_chain.add_pending_note(output_note);

    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Failure - sender_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDH_RECLAIM_HEIGHT_NOT_REACHED
    );

    // fast forward to reclaim block height + 1
    mock_chain
        .prove_until_block((reclaim_block_height.as_int() + 1) as u32)
        .unwrap();

    // CONSTRUCT AND EXECUTE TX (Success - sender_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute()
        .unwrap();

    let sender_account_after: Account = Account::from_parts(
        sender_account.id(),
        AssetVault::new(&[fungible_asset]).unwrap(),
        sender_account.storage().clone(),
        sender_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(
        executed_transaction_1.final_account().commitment(),
        sender_account_after.commitment()
    );

    Ok(())
}

#[test]
fn hybrid_p2id_script_timelock_test() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    // let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let code = fs::read_to_string(Path::new("../miden-lib/asm/note_scripts/P2IDH.masm")).unwrap();
    let serial_num = Word::default();
    let note_script = NoteScript::compile(code, assembler).unwrap();

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let reclaim_block_height = Felt::new(0); // if 0, means it is not reclaimable
    let timelock_block_height = Felt::new(7); // if 0 means it is not timelocked

    // Hybrid P2ID - P2ID(RT) Pay to Id, Optional Reclaimable & Timelockable
    let note_inputs = NoteInputs::new(vec![
        target_account.id().suffix(),
        target_account.id().prefix().into(),
        reclaim_block_height,
        timelock_block_height,
    ])
    .unwrap();

    let recipient = NoteRecipient::new(serial_num, note_script, note_inputs.clone());
    let tag = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;

    let fungible_asset: Asset = FungibleAsset::mock(100);
    let vault = NoteAssets::new(vec![fungible_asset])?;
    let hybrid_p2id = Note::new(vault, metadata, recipient);

    let output_note = OutputNote::Full(hybrid_p2id.clone());
    mock_chain.add_pending_note(output_note);

    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Failure - target_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(executed_transaction_1, ERR_P2IDH_TIMELOCK_NOT_REACHED);

    // fast forward to reclaim block height + 1
    mock_chain
        .prove_until_block((timelock_block_height.as_int() + 1) as u32)
        .unwrap();

    // CONSTRUCT AND EXECUTE TX (Success - target_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute()
        .unwrap();

    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset]).unwrap(),
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(
        executed_transaction_1.final_account().commitment(),
        target_account_after.commitment()
    );

    Ok(())
}

#[test]
fn hybrid_p2id_script_reclaimable_timelockable() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // ── create sender & target wallets ───────────────────────────────────────
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    // ── build note script ────────────────────────────────────────────────────
    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let code = fs::read_to_string(Path::new("../miden-lib/asm/note_scripts/P2IDH.masm")).unwrap();
    let note_script = NoteScript::compile(code, assembler).unwrap();

    let reclaim_block_height = Felt::new(10);
    let timelock_block_height = Felt::new(7);

    let note_inputs = NoteInputs::new(vec![
        target_account.id().suffix(),
        target_account.id().prefix().into(),
        reclaim_block_height,
        timelock_block_height,
    ])?;

    let recipient = NoteRecipient::new(Word::default(), note_script, note_inputs);
    let tag = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local)?;
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;

    let asset: Asset = FungibleAsset::mock(100);
    let vault = NoteAssets::new(vec![asset.into()])?;
    let hybrid_p2id = Note::new(vault, metadata, recipient);

    // push note on-chain
    mock_chain.add_pending_note(OutputNote::Full(hybrid_p2id.clone()));
    mock_chain.prove_next_block();

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDH_TIMELOCK_NOT_REACHED);

    // ───────────────────── early spend attempt (target)  → FAIL ─────────────
    let early_spend = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_spend, ERR_P2IDH_TIMELOCK_NOT_REACHED);

    // ───────────────────── advance chain past height 7 ──────────────────────
    mock_chain
        .prove_until_block((timelock_block_height.as_int() + 1) as u32)
        .unwrap();

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDH_RECLAIM_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past height 10 ──────────────────────
    mock_chain
        .prove_until_block((reclaim_block_height.as_int() + 1) as u32)
        .unwrap();

    // ───────────────────── target spends successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(target_account.id(), &[hybrid_p2id.id()], &[])
        .build()
        .execute()
        .unwrap();

    let target_after = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[asset])?,
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(final_tx.final_account().commitment(), target_after.commitment());

    Ok(())
}
