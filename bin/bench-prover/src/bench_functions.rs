use anyhow::Result;
use miden_objects::Felt;
use miden_objects::account::Account;
use miden_objects::asset::{Asset, AssetVault, FungibleAsset};
use miden_objects::note::NoteType;
use miden_objects::testing::account_id::ACCOUNT_ID_SENDER;
use miden_objects::transaction::ExecutedTransaction;
use miden_testing::{Auth, MockChain};
use miden_tx::{LocalTransactionProver, ProvingOptions, TransactionProver};

pub fn setup_consume_note_with_new_account() -> Result<ExecutedTransaction> {
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

    Ok(executed_transaction)
}

pub fn setup_consume_multiple_notes() -> Result<ExecutedTransaction> {
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

    let executed_transaction = tx_context.execute().unwrap();

    account.apply_delta(executed_transaction.account_delta()).unwrap();
    let resulting_asset = account.vault().assets().next().unwrap();
    if let Asset::Fungible(asset) = resulting_asset {
        assert_eq!(asset.amount(), 123u64);
    } else {
        panic!("Resulting asset should be fungible");
    }

    Ok(executed_transaction)
}

pub fn prove_transaction(executed_transaction: ExecutedTransaction) -> Result<()> {
    let executed_transaction_id = executed_transaction.id();

    let proof_options = ProvingOptions::default();
    let prover = LocalTransactionProver::new(proof_options);
    let proven_transaction: miden_objects::transaction::ProvenTransaction =
        prover.prove(executed_transaction.into()).unwrap();

    assert_eq!(proven_transaction.id(), executed_transaction_id);
    Ok(())
}
