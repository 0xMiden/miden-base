use anyhow::Context;
use miden_lib::errors::note_script_errors::{
    ERR_P2IDE_RECLAIM_ACCT_IS_NOT_SENDER, ERR_P2IDE_RECLAIM_DISABLED,
    ERR_P2IDE_RECLAIM_HEIGHT_NOT_REACHED, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED,
};
use miden_objects::{
    Felt,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    block::BlockNumber,
    note::{Note, NoteType},
};
use miden_testing::{Auth, MockChain};

use crate::assert_transaction_executor_error;

/// Test that the P2IDE note works like a regular P2ID note
#[test]
fn p2ide_script_success_without_reclaim_or_timelock() -> anyhow::Result<()> {
    let reclaim_height = None; // if 0, means it is not reclaimable
    let timelock_height = None; // if 0 means it is not timelocked

    let P2ideTestSetup {
        mock_chain,
        fungible_asset,
        target_account,
        malicious_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(reclaim_height, timelock_height)?;

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
    let reclaim_height = Some(BlockNumber::from(5u32));
    let timelock_height = None;

    let P2ideTestSetup {
        mut mock_chain,
        fungible_asset,
        target_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(reclaim_height, timelock_height)?;

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
    let reclaim_height = None;
    let timelock_height = BlockNumber::from(5u32);
    let P2ideTestSetup {
        mut mock_chain,
        fungible_asset,
        sender_account,
        target_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(reclaim_height, Some(timelock_height))?;

    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // ───────────────────── reclaim attempt (sender) → FAIL ────────────
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

    // ───────────────────── advance chain past unlock height block height ──────────────────────
    mock_chain.prove_until_block(timelock_height + 1)?;

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
    let reclaim_height = BlockNumber::from(1u32);
    let timelock_height = BlockNumber::from(5u32);

    let P2ideTestSetup {
        mut mock_chain,
        fungible_asset,
        sender_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(Some(reclaim_height), Some(timelock_height))?;

    // fast forward to reclaim block height + 2
    mock_chain
        .prove_until_block(reclaim_height + 2)
        .context("failed to prove multiple blocks")?;

    // CONSTRUCT AND EXECUTE TX (Failure - sender_account tries to reclaim)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED
    );

    mock_chain.prove_until_block(timelock_height)?;

    // CONSTRUCT AND EXECUTE TX (Success - sender_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
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
        executed_transaction_1.final_account().commitment(),
        sender_account_after.commitment()
    );

    Ok(())
}

/// Test that the P2IDE note can have timelock and reclaim functionality
#[test]
fn p2ide_script_reclaimable_timelockable() -> anyhow::Result<()> {
    let reclaim_height = BlockNumber::from(10u32);
    let timelock_height = BlockNumber::from(7u32);

    let P2ideTestSetup {
        mut mock_chain,
        fungible_asset,
        sender_account,
        target_account,
        malicious_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(Some(reclaim_height), Some(timelock_height))?;

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
    let reclaim_height = BlockNumber::from(5);
    let timelock_height = BlockNumber::from(3);

    let P2ideTestSetup {
        mut mock_chain,
        fungible_asset,
        sender_account,
        p2ide_note,
        ..
    } = setup_p2ide_test(Some(reclaim_height), Some(timelock_height))?;

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
        .build()?
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past reclaim height ──────────────────────
    mock_chain.prove_until_block(reclaim_height + 1)?;

    // ───────────────────── sender reclaims successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(sender_account.id(), &[p2ide_note.id()], &[])?
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

struct P2ideTestSetup {
    mock_chain: MockChain,
    fungible_asset: Asset,
    sender_account: Account,
    target_account: Account,
    malicious_account: Account,
    p2ide_note: Note,
}

fn setup_p2ide_test(
    reclaim_height: Option<BlockNumber>,
    timelock_height: Option<BlockNumber>,
) -> anyhow::Result<P2ideTestSetup> {
    let fungible_asset: Asset = FungibleAsset::mock(100);

    let mut builder = MockChain::builder();

    // Create sender and target accounts
    let sender_account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let target_account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let malicious_account = builder.add_existing_wallet(Auth::BasicAuth)?;

    let p2ide_note = builder.add_p2ide_note(
        sender_account.id(),
        target_account.id(),
        &[fungible_asset],
        NoteType::Public,
        reclaim_height,
        timelock_height,
    )?;

    let mock_chain = builder.build()?;

    Ok(P2ideTestSetup {
        mock_chain,
        fungible_asset,
        sender_account,
        target_account,
        malicious_account,
        p2ide_note,
    })
}
