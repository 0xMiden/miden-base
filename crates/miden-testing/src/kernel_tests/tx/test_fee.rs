use anyhow::Context;
use miden_objects::{self, Felt, Word, asset::FungibleAsset};

use crate::{Auth, MockChain};

// FEE TESTS
// ================================================================================================

/// Tests that a simple account can be created with non-zero fees.
#[test]
fn create_account_with_fees() -> anyhow::Result<()> {
    let amount = 10_000;
    let mut builder = MockChain::builder().verification_base_fee(50);
    let account = builder.create_new_wallet(Auth::IncrNonce)?;
    let fee_note = builder.add_p2id_note_with_fee(account.id(), amount)?;
    let chain = builder.build()?;

    let tx = chain
        .build_tx_context(account, &[fee_note.id()], &[])?
        .build()?
        .execute()
        .context("failed to execute account-creating transaction")?;

    let expected_fee = tx.compute_fee();
    assert_eq!(expected_fee, tx.fee().amount());

    // We expect that the new account contains the amount minus the paid fee.
    let mut added_asset = FungibleAsset::new(chain.native_asset_id(), amount)?;
    added_asset.sub(tx.fee().amount())?;

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
