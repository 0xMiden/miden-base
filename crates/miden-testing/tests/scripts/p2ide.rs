use anyhow::Context;
use miden_lib::{
    errors::note_script_errors::{
        ERR_P2IDE_RECLAIM_ACCT_IS_NOT_SENDER, ERR_P2IDE_RECLAIM_DISABLED,
        ERR_P2IDE_RECLAIM_HEIGHT_NOT_REACHED, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED,
    },
    note::create_p2ide_note,
};
use miden_objects::{
    Felt, Word,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    block::BlockNumber,
    crypto::rand::RpoRandomCoin,
    note::NoteType,
    transaction::OutputNote,
};
use miden_testing::{Auth, MockChain};

use crate::assert_transaction_executor_error;

/// Test that the P2IDE note works like a regular P2ID note
#[test]
fn p2ide_script_success_without_reclaim_or_timelock() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();

    // Create sender and target accounts
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let reclaim_height = None; // if 0, means it is not reclaimable
    let timelock_height = None; // if 0 means it is not timelocked
    let fungible_asset: Asset = FungibleAsset::mock(100);

    let p2ide_note = mock_chain.add_pending_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        reclaim_height,
        timelock_height,
    )?;

    mock_chain.prove_until_block(10u32).context("failed to prove multiple blocks")?;

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(malicious_account.id(), &[], &[p2ide_note.clone()])?
        .build()?
        .execute();

    assert_transaction_executor_error!(executed_transaction_1, ERR_P2IDE_RECLAIM_DISABLED);

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_2 = mock_chain
        .build_tx_context(target_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute()?;

    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset])?,
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );
    assert_eq!(
        executed_transaction_2.final_account().commitment(),
        target_account_after.commitment()
    );

    Ok(())
}

/// Test that the P2IDE note can have a timelock that unlocks before the reclaim block height
#[test]
fn p2ide_script_success_timelock_unlock_before_reclaim_height() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_height = BlockNumber::from(5u32);
    let timelock_height = None;

    let p2ide_note = mock_chain.add_pending_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        Some(reclaim_height),
        timelock_height,
    )?;

    mock_chain.prove_until_block(4).context("failed to prove multiple blocks")?;

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute()?;

    let target_account_after: Account = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset])?,
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

/// Test that the P2IDE note can have a timelock set and reclaim functionality
/// disabled.
#[test]
fn p2ide_script_timelocked_reclaim_disabled() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_height = None;
    let timelock_height = BlockNumber::from(5u32);

    let p2ide_note = mock_chain.add_pending_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        reclaim_height,
        Some(timelock_height),
    )?;

    mock_chain.prove_until_block(10)?;

    // ───────────────────── reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context_at(
            timelock_height.as_u32() - 1,
            sender_account.id(),
            &[p2ide_note.id()],
            &[],
        )?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── early spend attempt (target)  → FAIL ─────────────
    let early_spend = mock_chain
        .build_tx_context_at(
            timelock_height.as_u32() - 1,
            target_account.id(),
            &[p2ide_note.id()],
            &[],
        )?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_spend, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_RECLAIM_DISABLED);

    // ───────────────────── target spends successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(target_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute()?;

    let target_after = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset])?,
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(final_tx.final_account().commitment(), target_after.commitment());

    Ok(())
}

/// Test that an attempted reclaim of the P2IDE note fails if consumed by the creator
/// before the timelock expires. Creating a P2IDE note with a reclaim block height that is
/// less than the timelock block height would be the same as creating a P2IDE note
/// where the reclaim block height is equal to the timelock block height
#[test]
fn p2ide_script_reclaim_fails_before_timelock_expiry() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();

    // Create sender and target account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_height = BlockNumber::from(1u32);
    let timelock_height = BlockNumber::from(5u32);

    let p2ide_note = mock_chain.add_pending_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        Some(reclaim_height),
        Some(timelock_height),
    )?;

    mock_chain.prove_until_block(reclaim_height + 4)?;

    // CONSTRUCT AND EXECUTE TX (Failure - sender_account tries to reclaim)
    let executed_transaction_1 = mock_chain
        .build_tx_context_at(1, sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED
    );

    // CONSTRUCT AND EXECUTE TX (Success - sender_account)
    let executed_transaction_2 = mock_chain
        .build_tx_context_at(timelock_height, sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute()?;

    let sender_account_after: Account = Account::from_parts(
        sender_account.id(),
        AssetVault::new(&[fungible_asset])?,
        sender_account.storage().clone(),
        sender_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(
        executed_transaction_2.final_account().commitment(),
        sender_account_after.commitment()
    );

    Ok(())
}

/// Test that the P2IDE note can have timelock and reclaim functionality
#[test]
fn p2ide_script_reclaimable_timelockable() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_height = BlockNumber::from(10u32);
    let timelock_height = BlockNumber::from(7u32);

    let p2ide_note = mock_chain.add_pending_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        Some(reclaim_height),
        Some(timelock_height),
    )?;
    mock_chain.prove_next_block()?;
    mock_chain.prove_next_block()?;

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── early spend attempt (target)  → FAIL ─────────────
    let early_spend = mock_chain
        .build_tx_context(target_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_spend, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past timelock height ──────────────────────
    mock_chain.prove_until_block(timelock_height + 1)?;

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_RECLAIM_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past reclaim height ──────────────────────
    mock_chain.prove_until_block(reclaim_height + 1)?;

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(malicious_account.id(), &[], &[p2ide_note.clone()])?
        .build()?
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDE_RECLAIM_ACCT_IS_NOT_SENDER
    );

    // ───────────────────── target spends successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(target_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute()?;

    let target_after = Account::from_parts(
        target_account.id(),
        AssetVault::new(&[fungible_asset])?,
        target_account.storage().clone(),
        target_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(final_tx.final_account().commitment(), target_after.commitment());

    Ok(())
}

/// Test that the P2IDE note can be reclaimed after timelock
#[test]
fn p2ide_script_reclaim_success_after_timelock() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_block_height = 5;
    let timelock_block_height = 3;

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        Some(reclaim_block_height.into()),
        Some(timelock_block_height.into()),
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    // push note on-chain
    mock_chain.add_pending_note(OutputNote::Full(p2id_extended.clone()));
    mock_chain.prove_next_block()?;

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past reclaim height ──────────────────────
    mock_chain.prove_until_block(reclaim_block_height + 1)?;

    // ───────────────────── sender reclaims successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])?
        .build()?
        .execute()?;

    let sender_after = Account::from_parts(
        sender_account.id(),
        AssetVault::new(&[fungible_asset])?,
        sender_account.storage().clone(),
        sender_account.code().clone(),
        Felt::new(2),
    );

    assert_eq!(final_tx.final_account().commitment(), sender_after.commitment());

    Ok(())
}
