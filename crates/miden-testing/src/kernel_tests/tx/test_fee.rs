use anyhow::Context;
use assert_matches::assert_matches;
use miden_objects::account::AccountId;
use miden_objects::asset::FungibleAsset;
use miden_objects::testing::account_id::ACCOUNT_ID_NATIVE_ASSET_FAUCET;
use miden_objects::{self, Felt, Word};
use miden_tx::TransactionExecutorError;

use crate::{Auth, MockChain};

// FEE TESTS
// ================================================================================================

/// Tests that a simple wallet account can be created with non-zero fees.
#[test]
fn create_account_with_fees() -> anyhow::Result<()> {
    let note_amount = 10_000;

    let mut builder = MockChain::builder().verification_base_fee(50);
    let account = builder.create_new_wallet(Auth::IncrNonce)?;
    let fee_note = builder.add_p2id_note_with_fee(account.id(), note_amount)?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account, &[fee_note.id()], &[])?
        .build()?
        .execute_blocking()
        .context("failed to execute account-creating transaction")?;

    let expected_fee = tx.compute_fee();
    assert_eq!(expected_fee, tx.fee().amount());

    // We expect that the new account contains the amount minus the paid fee.
    let added_asset = FungibleAsset::new(chain.native_asset_id(), note_amount)?.sub(tx.fee())?;

    assert_eq!(tx.account_delta().nonce_delta(), Felt::new(1));
    // except for the nonce, the storage delta should be empty
    assert!(tx.account_delta().storage().is_empty());
    assert_eq!(tx.account_delta().vault().added_assets().count(), 1);
    assert_eq!(tx.account_delta().vault().removed_assets().count(), 0);
    assert_eq!(tx.account_delta().vault().added_assets().next().unwrap(), added_asset.into());
    assert_eq!(tx.final_account().nonce(), Felt::new(1));
    // account commitment should not be the empty word
    assert_ne!(tx.account_delta().to_commitment(), Word::empty());

    Ok(())
}

/// Tests that the transaction executor host aborts the transaction if the balance of the native
/// asset in the account does not cover the computed fee.
#[test]
fn tx_host_aborts_if_account_balance_does_not_cover_fee() -> anyhow::Result<()> {
    let account_amount = 100;
    let note_amount = 100;
    let native_asset_id = AccountId::try_from(ACCOUNT_ID_NATIVE_ASSET_FAUCET)?;

    let mut builder =
        MockChain::builder().native_asset_id(native_asset_id).verification_base_fee(50);
    let native_asset = FungibleAsset::new(native_asset_id, account_amount)?;
    let account =
        builder.add_existing_wallet_with_assets(Auth::IncrNonce, [native_asset.into()])?;
    let fee_note = builder.add_p2id_note_with_fee(account.id(), note_amount)?;
    let chain = builder.build()?;

    let err = chain
        .build_tx_context(account, &[fee_note.id()], &[])?
        .build()?
        .execute_blocking()
        .unwrap_err();

    assert_matches!(
        err,
        TransactionExecutorError::InsufficientFee { account_balance: _, tx_fee: _ }
    );

    Ok(())
}
