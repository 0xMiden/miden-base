use anyhow::Context;
use miden_lib::errors::note_script_errors::{
    ERR_P2IDR_RECLAIM_ACCT_IS_NOT_SENDER, ERR_P2IDR_RECLAIM_HEIGHT_NOT_REACHED,
};
use miden_objects::{
    Felt,
    account::Account,
    asset::{Asset, AssetVault, FungibleAsset},
    note::NoteType,
};
use miden_testing::{Auth, MockChain};

use crate::assert_transaction_executor_error;

#[test]
fn p2idr_script() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_until_block(3u32).context("failed to prove multiple blocks")?;

    // Create assets
    let fungible_asset: Asset = FungibleAsset::mock(100);

    // Create sender and target and malicious account
    let sender_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let target_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);
    let malicious_account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

    // Create the reclaim block heights
    let reclaim_block_height_in_time = 7.into();
    let reclaim_block_height_reclaimable = 2.into();

    // Create the notes with the P2IDR script
    let note_in_time = mock_chain
        .add_pending_p2id_note(
            sender_account.id(),
            target_account.id(),
            &[fungible_asset],
            NoteType::Public,
            Some(reclaim_block_height_in_time),
        )
        .unwrap();

    let note_reclaimable = mock_chain
        .add_pending_p2id_note(
            sender_account.id(),
            target_account.id(),
            &[fungible_asset],
            NoteType::Public,
            Some(reclaim_block_height_reclaimable),
        )
        .unwrap();

    mock_chain.prove_next_block();

    // --------------------------------------------------------------------------------------------
    // Case "in time": Only the target account can consume the note.
    // --------------------------------------------------------------------------------------------
    // CONSTRUCT AND EXECUTE TX (Success - Target Account)
    let executed_transaction_1 = mock_chain
        .build_tx_context(target_account.id(), &[note_in_time.id()], &[])
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

    // CONSTRUCT AND EXECUTE TX (Failure - Sender Account tries to consume too early)
    let executed_transaction_2 = mock_chain
        .build_tx_context(sender_account.id(), &[note_in_time.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_2,
        ERR_P2IDR_RECLAIM_HEIGHT_NOT_REACHED
    );

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account tries to consume)
    let executed_transaction_3 = mock_chain
        .build_tx_context(malicious_account.id(), &[note_in_time.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_3,
        ERR_P2IDR_RECLAIM_ACCT_IS_NOT_SENDER
    );

    // --------------------------------------------------------------------------------------------
    // Case "reclaimable": Both target and sender accounts can consume the note.
    // --------------------------------------------------------------------------------------------
    // CONSTRUCT AND EXECUTE TX (Success - Target Account consumes reclaimable note)
    let executed_transaction_4 = mock_chain
        .build_tx_context(target_account.id(), &[note_reclaimable.id()], &[])
        .build()
        .execute()
        .unwrap();

    assert_eq!(executed_transaction_4.account_delta().nonce(), Some(Felt::new(2)));

    // CONSTRUCT AND EXECUTE TX (Success - Sender Account consumes reclaimable note)
    let executed_transaction_5 = mock_chain
        .build_tx_context(sender_account.id(), &[note_reclaimable.id()], &[])
        .build()
        .execute()
        .unwrap();

    assert_eq!(executed_transaction_5.account_delta().nonce(), Some(Felt::new(2)));

    // CONSTRUCT AND EXECUTE TX (Failure - Malicious Account tries to consume reclaimable note)
    let executed_transaction_6 = mock_chain
        .build_tx_context(malicious_account.id(), &[note_reclaimable.id()], &[])
        .build()
        .execute();

    assert_transaction_executor_error!(
        executed_transaction_6,
        ERR_P2IDR_RECLAIM_ACCT_IS_NOT_SENDER
    );

    Ok(())
}
