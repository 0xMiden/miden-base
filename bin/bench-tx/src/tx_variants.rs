extern crate alloc;

use anyhow::Result;
use miden_lib::note::create_p2id_note;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::Account;
use miden_objects::asset::{Asset, AssetVault, FungibleAsset};
use miden_objects::crypto::rand::RpoRandomCoin;
use miden_objects::note::NoteType;
use miden_objects::testing::account_id::ACCOUNT_ID_SENDER;
use miden_objects::transaction::{ExecutedTransaction, OutputNote};
use miden_objects::{Felt, Word};
use miden_testing::{Auth, MockChain};

/// Runs the transaction which creates a single P2ID note.
pub fn tx_create_p2id() -> anyhow::Result<ExecutedTransaction> {
    let mut builder = MockChain::builder();

    let fungible_asset: Asset = FungibleAsset::mock(100);

    let account = builder.add_existing_wallet_with_assets(Auth::BasicAuth, [fungible_asset])?;

    let mock_chain = builder.build()?;

    let output_note = create_p2id_note(
        account.id(),
        account.id(),
        vec![fungible_asset],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    let tx_note_creation_script = format!(
        "
        use.miden::output_note
        use.std::sys

        begin
            # create an output note with fungible asset
            push.{RECIPIENT}
            push.{note_execution_hint}
            push.{note_type}
            push.0              # aux
            push.{tag}
            call.output_note::create
            # => [note_idx]

            # move the asset to the note
            push.{asset}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw
            # => [note_idx]

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        RECIPIENT = output_note.recipient().digest(),
        note_execution_hint = Felt::from(output_note.metadata().execution_hint()),
        note_type = NoteType::Public as u8,
        tag = output_note.metadata().tag(),
        asset = Word::from(fungible_asset),
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(tx_note_creation_script)?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .tx_script(tx_script)
        .build()?;

    Ok(tx_context.execute_blocking()?)
}

/// Runs the transaction which consumes a P2ID note into a new basic wallet.
pub fn tx_consume_p2id() -> Result<ExecutedTransaction> {
    // Create assets
    let fungible_asset: Asset = FungibleAsset::mock(123);

    let mut builder = MockChain::builder();

    // Create target account
    let target_account = builder.create_new_wallet(Auth::BasicAuth)?;

    // Create the note
    let note = builder
        .add_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            target_account.id(),
            &[fungible_asset],
            NoteType::Public,
        )
        .unwrap();

    let mock_chain = builder.build()?;

    // construct and execute transaction
    let executed_transaction = mock_chain
        .build_tx_context(target_account.clone(), &[note.id()], &[])?
        .build()?
        .execute_blocking()?;

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

    Ok(executed_transaction)
}

/// Runs the transaction which consumes multiple P2ID notes into an existing basic wallet.
pub fn tx_consume_multiple_p2id_notes() -> Result<ExecutedTransaction> {
    let mut builder = MockChain::builder();

    let mut account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let fungible_asset_1: Asset = FungibleAsset::mock(100);
    let fungible_asset_2: Asset = FungibleAsset::mock(23);

    let note_1 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[fungible_asset_1],
        NoteType::Private,
    )?;
    let note_2 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[fungible_asset_2],
        NoteType::Private,
    )?;

    let mock_chain = builder.build()?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[note_1.id(), note_2.id()], &[])?
        .build()?;

    let executed_transaction = tx_context.execute_blocking().unwrap();

    account.apply_delta(executed_transaction.account_delta()).unwrap();
    let resulting_asset = account.vault().assets().next().unwrap();
    if let Asset::Fungible(asset) = resulting_asset {
        assert_eq!(asset.amount(), 123u64);
    } else {
        panic!("Resulting asset should be fungible");
    }

    Ok(executed_transaction)
}
