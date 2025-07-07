use anyhow::Result;
use miden_objects::{
    Felt,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    note::NoteType,
    testing::account_id::ACCOUNT_ID_SENDER,
    transaction::ExecutedTransaction,
};
use miden_testing::{Auth, MockChain};
use miden_tx::{LocalTransactionProver, ProvingOptions, TransactionProver};

pub fn setup_consume_note_with_new_account() -> Result<ExecutedTransaction> {
    let mut mock_chain = MockChain::new();

    // Create assets
    let fungible_asset: Asset = FungibleAsset::mock(123);

    // Create target account
    let target_account = mock_chain.add_pending_new_wallet(Auth::BasicAuth);

    // Create the note
    let note = mock_chain
        .add_pending_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            target_account.id(),
            &[fungible_asset],
            NoteType::Public,
        )
        .unwrap();

    mock_chain.prove_next_block()?;

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------

    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(target_account.id(), &[note.id()], &[])?
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
    let mut mock_chain = MockChain::new();
    let mut account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    let fungible_asset_1: Asset = FungibleAsset::mock(100);
    let fungible_asset_2: Asset = FungibleAsset::mock(23);

    let note_1 = mock_chain
        .add_pending_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            account.id(),
            &[fungible_asset_1],
            NoteType::Private,
        )
        .unwrap();
    let note_2 = mock_chain
        .add_pending_p2id_note(
            ACCOUNT_ID_SENDER.try_into().unwrap(),
            account.id(),
            &[fungible_asset_2],
            NoteType::Private,
        )
        .unwrap();

    mock_chain.prove_next_block()?;
    mock_chain.prove_next_block()?;

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
