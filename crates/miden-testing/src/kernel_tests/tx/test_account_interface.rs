use alloc::vec::Vec;

use assert_matches::assert_matches;
use miden_lib::note::well_known_note::{NoteConsumptionStatus, WellKnownNote};
use miden_lib::note::{create_p2id_note, create_p2ide_note};
use miden_lib::testing::mock_account::MockAccountExt;
use miden_lib::testing::note::NoteBuilder;
use miden_lib::transaction::TransactionKernel;
use miden_objects::account::{Account, AccountId};
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::block::BlockNumber;
use miden_objects::crypto::rand::FeltRng;
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    ACCOUNT_ID_SENDER,
};
use miden_objects::transaction::OutputNote;
use miden_objects::{Felt, Word, ZERO};
use miden_processor::ExecutionError;
use miden_processor::crypto::RpoRandomCoin;
use miden_tx::auth::UnreachableAuth;
use miden_tx::{
    FailedNote,
    NoteConsumptionChecker,
    NoteConsumptionInfo,
    TransactionExecutor,
    TransactionExecutorError,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::utils::create_public_p2any_note;
use crate::{Auth, MockChain, TransactionContextBuilder, TxContextInput};

#[tokio::test]
async fn check_note_consumability_well_known_notes_success() -> anyhow::Result<()> {
    let p2id_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([2u32; 4])),
    )?;

    let p2ide_note = create_p2ide_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        None,
        None,
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([2u32; 4])),
    )?;

    let notes = vec![p2id_note, p2ide_note];
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .extend_input_notes(notes.clone())
        .build()?;

    let target_account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumption_info = notes_checker
        .check_notes_consumability(target_account_id, block_ref, notes.clone(), tx_args)
        .await?;

    assert_matches!(consumption_info, NoteConsumptionInfo { successful, failed, .. } => {
        assert_eq!(successful.len(), notes.len());

        // we asserted that `successful` and `notes` vectors have the same length, so it's safe to
        // check their equality that way
        successful.iter().for_each(|successful_note| assert!(notes.contains(successful_note)));

        assert!(failed.is_empty());
    });

    Ok(())
}

#[rstest::rstest]
#[case::one(vec![create_public_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), [FungibleAsset::mock(100)])])]
#[tokio::test]
async fn check_note_consumability_custom_notes_success(
    #[case] notes: Vec<Note>,
) -> anyhow::Result<()> {
    let tx_context = {
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, Auth::IncrNonce);
        let (_, authenticator) = Auth::BasicAuth.build_component();
        TransactionContextBuilder::new(account)
            .extend_input_notes(notes.clone())
            .authenticator(authenticator)
            .build()?
    };

    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor = TransactionExecutor::new(&tx_context)
        .with_authenticator(tx_context.authenticator().unwrap())
        .with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumption_info = notes_checker
        .check_notes_consumability(account_id, block_ref, notes.clone(), tx_args)
        .await?;

    assert_matches!(consumption_info, NoteConsumptionInfo { successful, failed, .. }=> {
        if notes.is_empty() {
            assert!(successful.is_empty());
            assert!(failed.is_empty());
        } else {
            assert_eq!(successful.len(), notes.len());
        }
    });
    Ok(())
}

#[tokio::test]
async fn check_note_consumability_partial_success() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(Auth::IncrNonce)?;

    let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let failing_note_1 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.0 div end")
    .dynamically_linked_libraries([TransactionKernel::library()])
    .build()?;

    let failing_note_2 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.2 drop push.0 div end")
    .dynamically_linked_libraries([TransactionKernel::library()])
    .build()?;

    let successful_note_1 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(10)],
        NoteType::Public,
    )?;

    let successful_note_2 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(145)],
        NoteType::Public,
    )?;

    let successful_note_3 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(250)],
        NoteType::Public,
    )?;

    let mock_chain = builder.build()?;
    let notes = vec![
        successful_note_2.clone(),
        successful_note_1.clone(),
        failing_note_2.clone(),
        failing_note_1.clone(),
        successful_note_3.clone(),
    ];
    let tx_context = mock_chain
        .build_tx_context(TxContextInput::Account(account), &[], &notes)?
        .build()?;

    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumption_info = notes_checker
        .check_notes_consumability(account_id, block_ref, notes, tx_args)
        .await?;

    assert_matches!(
        consumption_info,
        NoteConsumptionInfo {
            successful,
            failed
        } => {
                assert_eq!(failed.len(), 2);
                assert_eq!(successful.len(), 3);

                // First failing note.
                assert_matches!(
                    failed.first().expect("first failed notes should exist"),
                    FailedNote {
                        note,
                        error: TransactionExecutorError::TransactionProgramExecutionFailed(
                            ExecutionError::DivideByZero { .. })
                    } => {
                        assert_eq!(
                            note.id(),
                            failing_note_2.id(),
                        );
                    }
                );
                // Second failing note.
                assert_matches!(
                    failed.get(1).expect("second failed note should exist"),
                    FailedNote {
                        note,
                        error: TransactionExecutorError::TransactionProgramExecutionFailed(
                            ExecutionError::DivideByZero { .. })
                    } => {
                        assert_eq!(
                            note.id(),
                            failing_note_1.id(),
                        );
                    }
                );
                // Successful notes.
                assert_eq!(
                    [successful[0].id(), successful[1].id(), successful[2].id()],
                    [successful_note_2.id(), successful_note_1.id(), successful_note_3.id()],
                );
            }
    );
    Ok(())
}

#[tokio::test]
async fn check_note_consumability_epilogue_failure() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Use basic auth which will cause epilogue failure when paired up with unreachable auth.
    let account = builder.add_existing_wallet(Auth::BasicAuth)?;

    let successful_note = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(10)],
        NoteType::Public,
    )?;

    let mock_chain = builder.build()?;
    let notes = vec![successful_note.clone()];
    let tx_context = mock_chain
        .build_tx_context(TxContextInput::Account(account), &[], &notes)?
        .build()?;

    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    // Use an auth that fails in order to force an epilogue failure when paired up with basic auth.
    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumption_info = notes_checker
        .check_notes_consumability(account_id, block_ref, notes, tx_args)
        .await?;

    assert_matches!(
       consumption_info,
       NoteConsumptionInfo {
           successful,
           failed
       } => {
           assert!(successful.is_empty());
           assert_eq!(failed.len(), 1);
       }
    );
    Ok(())
}

#[tokio::test]
async fn check_note_consumability_epilogue_failure_with_new_combination() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(Auth::IncrNonce)?;

    // Prepare set of notes expected to succeed despite the fact that they will be grouped with
    // notes that cause epilogue failure and transaction execution failure. The epilogue failure
    // in particular will cause the note checker to execute
    // `find_largest_executable_combination()` which this test is mainly concerned about.
    let successful_note_1 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(10)],
        NoteType::Public,
    )?;
    let successful_note_2 = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(145)],
        NoteType::Public,
    )?;
    let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();
    let successful_note_3 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.1 div end")
    .dynamically_linked_libraries([TransactionKernel::library()])
    .build()?;
    let failing_note_1 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.0 div end")
    .dynamically_linked_libraries([TransactionKernel::library()])
    .build()?;

    // Create a note that causes epilogue failure. Adds assets to the transaction without moving
    // them anywhere which causes an "asset imbalance" that violates the asset preservation rules.
    let note_asset = FungibleAsset::mock(700).unwrap_fungible();
    let fail_epilogue_note = NoteBuilder::new(account.id(), &mut rand::rng())
        .add_assets([Asset::from(note_asset)])
        .build()?;
    builder.add_output_note(OutputNote::Full(fail_epilogue_note.clone()));

    let mock_chain = builder.build()?;
    let notes = vec![
        successful_note_1.clone(),
        fail_epilogue_note.clone(),
        successful_note_2.clone(),
        failing_note_1.clone(),
        successful_note_3.clone(),
    ];
    let tx_context = mock_chain
        .build_tx_context(TxContextInput::Account(account), &[], &notes)?
        .build()?;

    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumption_info = notes_checker
        .check_notes_consumability(account_id, block_ref, notes, tx_args)
        .await?;

    assert_matches!(
        consumption_info,
        NoteConsumptionInfo {
            successful,
            failed
        } => {
                assert_eq!(failed.len(), 2);
                assert_eq!(successful.len(), 3);

                // First failing note should be the note that does not cause epilogue failure.
                assert_matches!(
                    failed.first().expect("first failed notes should exist"),
                    FailedNote {
                        note,
                        error: TransactionExecutorError::TransactionProgramExecutionFailed(
                            ExecutionError::DivideByZero { .. })
                    } => {
                        assert_eq!(
                            note.id(),
                            failing_note_1.id(),
                        );
                    }
                );
                // Second failing note should be the note that causes epilogue failure.
                assert_matches!(
                    failed.get(1).expect("second failed note should exist"),
                    FailedNote {
                        note,
                        error: TransactionExecutorError::TransactionProgramExecutionFailed(
                            ExecutionError::FailedAssertion { .. })
                    } => {
                        assert_eq!(
                            note.id(),
                            fail_epilogue_note.id(),
                        );
                    }
                );
                // Successful notes.
                assert_eq!(
                    [successful[0].id(), successful[1].id(), successful[2].id()],
                    [successful_note_1.id(), successful_note_2.id(), successful_note_3.id()],
                );
            }
    );
    Ok(())
}

#[tokio::test]
async fn test_check_note_consumability_without_signatures() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Use basic auth which will cause epilogue failure when paired up with unreachable auth.
    let account = builder.add_existing_wallet(Auth::BasicAuth)?;

    let successful_note = builder.add_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(10)],
        NoteType::Public,
    )?;

    let mock_chain = builder.build()?;
    let notes = vec![successful_note.clone()];
    let tx_context = mock_chain
        .build_tx_context(TxContextInput::Account(account), &[], &notes)?
        .build()?;

    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    // Use an auth that fails in order to force an epilogue failure when paired up with basic auth.
    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let consumability_info: NoteConsumptionStatus = notes_checker
        .can_consume(account_id, block_ref, successful_note, tx_args)
        .await?;

    assert_eq!(consumability_info, NoteConsumptionStatus::ConsumableWithAuthorization);

    Ok(())
}

/// Tests the correctness of the [`NoteConsumptionChecker::can_consume()`] procedure in scenarios
/// where static analysis should guarantee that the note cannot be consumed.
#[tokio::test]
async fn test_check_note_consumability_static_analysis() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let account = builder.add_existing_wallet(Auth::Noop)?;
    let target_account_id = account.id();
    let sender_account_id = ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap();

    // create notes for testing
    // --------------------------------------------------------------------------------------------
    let p2ide_wrong_inputs_number = create_p2ide_note_with_inputs([1, 2, 3], sender_account_id);
    builder.add_output_note(OutputNote::Full(p2ide_wrong_inputs_number.clone()));

    let p2ide_invalid_target_id = create_p2ide_note_with_inputs([1, 2, 3, 4], sender_account_id);
    builder.add_output_note(OutputNote::Full(p2ide_invalid_target_id.clone()));

    let p2ide_timelock_not_reached = create_p2ide_note_with_inputs(
        [target_account_id.suffix().as_int(), target_account_id.prefix().as_u64(), 3, 10],
        sender_account_id,
    );
    builder.add_output_note(OutputNote::Full(p2ide_timelock_not_reached.clone()));

    // swap target and sender account IDs to emulate the recall attempt scenario
    let p2ide_recall_not_reached = create_p2ide_note_with_inputs(
        [sender_account_id.suffix().as_int(), sender_account_id.prefix().as_u64(), 3, 0],
        target_account_id,
    );
    builder.add_output_note(OutputNote::Full(p2ide_recall_not_reached.clone()));

    // finalize mock chain and create notes checker
    // --------------------------------------------------------------------------------------------
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::Account(account),
            &[],
            &vec![p2ide_wrong_inputs_number.clone(), p2ide_timelock_not_reached.clone()],
        )?
        .build()?;

    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    // check the note with invalid number of inputs
    // --------------------------------------------------------------------------------------------
    let consumability_info: NoteConsumptionStatus = notes_checker
        .can_consume(
            target_account_id,
            block_ref,
            p2ide_wrong_inputs_number.clone(),
            tx_args.clone(),
        )
        .await?;
    assert_matches!(consumability_info, NoteConsumptionStatus::Incompatible(reason) => {
        assert_eq!(reason, format!(
                        "P2IDE note should have 4 inputs, but {} was provided",
                        p2ide_wrong_inputs_number.recipient().inputs().num_values()
                    ));
    });

    // check the note with invalid target account ID
    // --------------------------------------------------------------------------------------------
    let consumability_info: NoteConsumptionStatus = notes_checker
        .can_consume(target_account_id, block_ref, p2ide_invalid_target_id.clone(), tx_args.clone())
        .await?;
    assert_matches!(consumability_info, NoteConsumptionStatus::Incompatible(reason) => {
        assert_eq!(reason, "Account ID provided to the P2IDE note inputs is invalid");
    });

    // check the note with timelock
    // --------------------------------------------------------------------------------------------
    let consumability_info: NoteConsumptionStatus = notes_checker
        .can_consume(
            target_account_id,
            block_ref,
            p2ide_timelock_not_reached.clone(),
            tx_args.clone(),
        )
        .await?;
    assert_eq!(
        consumability_info,
        NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(10))
    );

    // check the note with unreached recall height
    // --------------------------------------------------------------------------------------------
    let consumability_info: NoteConsumptionStatus = notes_checker
        .can_consume(
            target_account_id,
            block_ref,
            p2ide_recall_not_reached.clone(),
            tx_args.clone(),
        )
        .await?;
    assert_eq!(consumability_info, NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(3)));

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

/// Creates a mock P2IDE note with the specified note inputs.
fn create_p2ide_note_with_inputs(inputs: impl IntoIterator<Item = u64>, sender: AccountId) -> Note {
    let serial_num = RpoRandomCoin::new(Default::default()).draw_word();
    let note_script = WellKnownNote::P2IDE.script();
    let recipient = NoteRecipient::new(
        serial_num,
        note_script,
        NoteInputs::new(inputs.into_iter().map(Felt::new).collect()).unwrap(),
    );

    let tag = NoteTag::from_account_id(sender);
    let metadata =
        NoteMetadata::new(sender, NoteType::Public, tag, NoteExecutionHint::always(), ZERO)
            .unwrap();

    Note::new(NoteAssets::default(), metadata, recipient)
}
