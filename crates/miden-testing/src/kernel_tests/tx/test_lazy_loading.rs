//! This module tests lazy loading.
//!
//! Once lazy loading is enabled generally, it can be removed and/or integrated into other tests.

use miden_lib::testing::note::NoteBuilder;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::AccountId;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_NATIVE_ASSET_FAUCET,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
};
use miden_objects::testing::constants::FUNGIBLE_ASSET_AMOUNT;

use super::Word;
use crate::{Auth, MockChain, TransactionContextBuilder};

/// Tests that adding two different assets to the account vault succeeds when lazy loading is
/// enabled.
#[test]
fn adding_fungible_assets_with_lazy_loading_succeeds() -> anyhow::Result<()> {
    let faucet_id1: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let faucet_id2: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().unwrap();

    let fungible_asset1 =
        FungibleAsset::new(faucet_id1, FungibleAsset::MAX_AMOUNT - FUNGIBLE_ASSET_AMOUNT)?;
    let fungible_asset2 = FungibleAsset::new(faucet_id2, FUNGIBLE_ASSET_AMOUNT)?;

    // Build a note that adds the assets to the input vault of the transaction. This is necessary
    // to adhere to asset preservation rules.
    let asset_note = NoteBuilder::new(faucet_id1, rand::rng())
        .add_assets([fungible_asset1, fungible_asset2].map(Asset::from))
        .build()?;

    let code = format!(
        "
      use.mock::account

      begin
          push.{FUNGIBLE_ASSET1}
          call.account::add_asset dropw

          push.{FUNGIBLE_ASSET2}
          call.account::add_asset dropw
      end
      ",
        FUNGIBLE_ASSET1 = Word::from(fungible_asset1),
        FUNGIBLE_ASSET2 = Word::from(fungible_asset2)
    );

    let builder = ScriptBuilder::with_mock_libraries()?;
    let source_manager = builder.source_manager();
    let tx_script = builder.compile_tx_script(code)?;
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script)
        .extend_input_notes(vec![asset_note])
        .enable_partial_loading()
        .with_source_manager(source_manager)
        .build()?;
    let account = tx_context.account().clone();
    let tx = tx_context.execute_blocking()?;

    let mut account_vault = account.vault().clone();
    account_vault.add_asset(fungible_asset1.into())?;
    account_vault.add_asset(fungible_asset2.into())?;

    assert_eq!(tx.final_account().vault_root(), account_vault.root());

    Ok(())
}

/// Tests that removing two different assets from the account vault succeeds when lazy loading is
/// enabled.
#[test]
fn removing_fungible_assets_with_lazy_loading_succeeds() -> anyhow::Result<()> {
    let faucet_id1: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let faucet_id2: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().unwrap();

    let fungible_asset1 =
        FungibleAsset::new(faucet_id1, FungibleAsset::MAX_AMOUNT - FUNGIBLE_ASSET_AMOUNT)?;
    let fungible_asset2 = FungibleAsset::new(faucet_id2, FUNGIBLE_ASSET_AMOUNT)?;

    let code = format!(
        "
      use.mock::account
      use.mock::util

      begin
          push.{FUNGIBLE_ASSET1}
          call.account::remove_asset
          # => []

          # move asset to note to adhere to asset preservation rules
          exec.util::create_random_note_with_asset drop
          # => []

          push.{FUNGIBLE_ASSET2}
          call.account::remove_asset
          # => [ASSET]

          # move asset to note to adhere to asset preservation rules
          exec.util::create_random_note_with_asset drop
          # => []
      end
      ",
        FUNGIBLE_ASSET1 = Word::from(fungible_asset1),
        FUNGIBLE_ASSET2 = Word::from(fungible_asset2)
    );

    let builder = ScriptBuilder::with_mock_libraries()?;
    let source_manager = builder.source_manager();
    let tx_script = builder.compile_tx_script(code)?;

    let mut builder = MockChain::builder();
    let account = builder.add_existing_mock_account_with_assets(
        crate::Auth::IncrNonce,
        [fungible_asset1, fungible_asset2].map(Asset::from),
    )?;
    let tx_context = builder
        .build()?
        .build_tx_context(account, &[], &[])?
        .tx_script(tx_script)
        .enable_partial_loading()
        .with_source_manager(source_manager)
        .build()?;
    let account = tx_context.account().clone();
    let tx = tx_context.execute_blocking()?;

    let mut account_vault = account.vault().clone();
    account_vault.remove_asset(fungible_asset1.into())?;
    account_vault.remove_asset(fungible_asset2.into())?;

    assert_eq!(tx.final_account().vault_root(), account_vault.root());

    Ok(())
}

/// Tests that a transaction against an account with a non-empty vault successfully loads the fee
/// asset during the epilogue.
///
/// The non-empty vault is important for the test because the advice provider's merkle store has all
/// merkle paths for an empty vault by default, and so there would be nothing to load.
#[test]
fn loading_fee_asset_succeeds() -> anyhow::Result<()> {
    let mut builder =
        MockChain::builder().native_asset_id(ACCOUNT_ID_NATIVE_ASSET_FAUCET.try_into()?);
    let account = builder.add_existing_mock_account_with_assets(
        Auth::IncrNonce,
        [
            FungibleAsset::mock(23),
            FungibleAsset::new(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into()?, 50)?.into(),
        ],
    )?;
    builder
        .build()?
        .build_tx_context(account, &[], &[])?
        .enable_partial_loading()
        .build()?
        .execute_blocking()?;

    Ok(())
}
