use anyhow::Context;
use miden_lib::{
    errors::note_script_errors::{
        ERR_P2IDE_RECLAIM_ACCT_IS_NOT_SENDER, ERR_P2IDE_RECLAIM_DISABLED,
        ERR_P2IDE_RECLAIM_HEIGHT_NOT_REACHED, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED,
    },
    note::create_p2ide_note,
};
use miden_objects::{
    Felt, ONE,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
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
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target accounts
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let recall_height = None; // if 0, means it is not reclaimable
    let timelock_height = None; // if 0 means it is not timelocked

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        recall_height,
        timelock_height,
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let output_note = OutputNote::Full(p2id_extended.clone());
    mock_chain.add_pending_note(output_note);
    mock_chain.prove_next_block();

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(malicious_account.id(), &[], &[p2id_extended.clone()])
        .build()
        .execute();

    assert_transaction_executor_error!(executed_transaction_1, ERR_P2IDE_RECLAIM_DISABLED);

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_2 = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
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

    let reclaim_block_height = 5;
    let timelock_block_height = None;

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        Some(reclaim_block_height.into()),
        timelock_block_height,
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let output_note = OutputNote::Full(p2id_extended.clone());
    mock_chain.add_pending_note(output_note);
    mock_chain.prove_until_block(4).context("failed to prove multiple blocks")?;

    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
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

/// Test that the P2IDE note can have a timelock set and reclaim functionality
/// disabled.
#[test]
fn p2ide_script_timelocked_reclaim_disabled() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_block_height = None;
    let timelock_block_height = 5;

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        reclaim_block_height,
        Some(timelock_block_height.into()),
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    // push note on-chain
    mock_chain.add_pending_note(OutputNote::Full(p2id_extended.clone()));
    mock_chain.prove_next_block();

    // ───────────────────── reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── early spend attempt (target)  → FAIL ─────────────
    let early_spend = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_spend, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past unlock height block height ──────────────────────
    mock_chain.prove_until_block(timelock_block_height + 1).unwrap();

    // ───────────────────── reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_RECLAIM_DISABLED);

    // ───────────────────── target spends successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute()
        .unwrap();

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
    mock_chain.prove_until_block(1u32).context("failed to prove multiple blocks")?;

    // Create sender and target account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let reclaim_block_height = 1;
    let timelock_block_height = 5;

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        Some(reclaim_block_height.into()),
        Some(timelock_block_height.into()),
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let output_note = OutputNote::Full(p2id_extended.clone());
    mock_chain.add_pending_note(output_note);
    mock_chain.prove_next_block();

    // fast forward to reclaim block height + 2
    mock_chain
        .prove_until_block(reclaim_block_height + 2)
        .context("failed to prove multiple blocks")?;

    // CONSTRUCT AND EXECUTE TX (Failure - sender_account tries to reclaim)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED
    );

    mock_chain.prove_until_block(timelock_block_height).unwrap();

    // CONSTRUCT AND EXECUTE TX (Success - sender_account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
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

    let reclaim_block_height = 10;
    let timelock_block_height = 7;

    let p2id_extended = create_p2ide_note(
        sender_account.id(),
        target_account.id(),
        vec![fungible_asset],
        Some(reclaim_block_height.into()),
        Some(timelock_block_height.into()),
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    // push note on-chain
    mock_chain.add_pending_note(OutputNote::Full(p2id_extended.clone()));
    mock_chain.prove_next_block();

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── early spend attempt (target)  → FAIL ─────────────
    let early_spend = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_spend, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past timelock height ──────────────────────
    mock_chain.prove_until_block(timelock_block_height + 1).unwrap();

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_RECLAIM_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past reclaim height ──────────────────────
    mock_chain.prove_until_block(reclaim_block_height + 1).unwrap();

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(malicious_account.id(), &[], &[p2id_extended.clone()])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_1,
        ERR_P2IDE_RECLAIM_ACCT_IS_NOT_SENDER
    );

    // ───────────────────── target spends successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(target_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute()
        .unwrap();

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
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    // push note on-chain
    mock_chain.add_pending_note(OutputNote::Full(p2id_extended.clone()));
    mock_chain.prove_next_block();

    // ───────────────────── early reclaim attempt (sender) → FAIL ────────────
    let early_reclaim = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(early_reclaim, ERR_P2IDE_TIMELOCK_HEIGHT_NOT_REACHED);

    // ───────────────────── advance chain past reclaim height ──────────────────────
    mock_chain.prove_until_block(reclaim_block_height + 1).unwrap();

    // ───────────────────── sender reclaims successfully ───────────────────────
    let final_tx = mock_chain
        .build_tx_context(sender_account.id(), &[p2id_extended.id()], &[])
        .build()
        .execute()
        .unwrap();

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
