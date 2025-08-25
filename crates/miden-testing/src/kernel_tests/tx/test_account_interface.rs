use alloc::vec::Vec;

use assert_matches::assert_matches;
use miden_lib::note::{create_p2id_note, create_p2ide_note};
use miden_lib::testing::mock_account::MockAccountExt;
use miden_lib::transaction::TransactionKernel;
use miden_objects::Word;
use miden_objects::account::{Account, AccountId};
use miden_objects::asset::FungibleAsset;
use miden_objects::note::{Note, NoteType};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    ACCOUNT_ID_SENDER,
};
use miden_objects::testing::note::NoteBuilder;
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

use crate::utils::create_p2any_note;
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

    let notes = vec![p2ide_note, p2id_note];
    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .extend_input_notes(notes.clone())
        .build()?;

    let input_notes = tx_context.input_notes().clone();
    let target_account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(target_account_id, block_ref, input_notes, tx_args)
        .await?;

    assert_matches!(execution_check_result, NoteConsumptionInfo { successful, failed, .. } => {
        assert_eq!(successful.len(), notes.len());
        successful.iter().zip(notes.iter()).for_each(|(success, note)| {
            assert_eq!(success, note);
        });
        assert!(failed.is_empty());
    });

    Ok(())
}

#[rstest::rstest]
#[case::empty(vec![])]
#[case::one(vec![create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)])])]
#[tokio::test]
async fn check_note_consumability_custom_notes_success(
    #[case] notes: Vec<Note>,
) -> anyhow::Result<()> {
    let tx_context = {
        let account =
            Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, Auth::IncrNonce);
        TransactionContextBuilder::new(account)
            .extend_input_notes(notes.clone())
            .build()?
    };

    let input_notes = tx_context.input_notes().clone();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(account_id, block_ref, input_notes, tx_args)
        .await?;

    assert_matches!(execution_check_result, NoteConsumptionInfo { successful, failed, .. }=> {
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
async fn check_note_consumability_failure() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let mock_chain = builder.build()?;

    let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let failing_note_1 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.0 div end")
    .build(&TransactionKernel::with_kernel_library())?;

    let failing_note_2 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.2 drop push.0 div end")
    .build(&TransactionKernel::with_kernel_library())?;

    let successful_note_1 = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([2u32; 4])),
    )?;

    let successful_note_2 = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        account.id(),
        vec![FungibleAsset::mock(145)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([2u32; 4])),
    )?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::Account(account),
            &[],
            &[
                successful_note_2.clone(),
                successful_note_1.clone(),
                failing_note_2.clone(),
                failing_note_1,
            ],
        )?
        .build()?;

    let input_notes = tx_context.input_notes().clone();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(account_id, block_ref, input_notes, tx_args)
        .await?;

    assert_matches!(
        execution_check_result,
        NoteConsumptionInfo {
            successful,
            failed,
            ..
        } => {
                assert_matches!(
                    failed.first().expect("failed notes should exist"),
                    FailedNote {
                        note,
                        error: TransactionExecutorError::TransactionProgramExecutionFailed(
                            ExecutionError::DivideByZero { .. }),
                        ..
                    } => {
                        assert_eq!(
                            note.id(),
                            failing_note_2.id(),
                        );
                    }
                );
                assert_eq!(
                    [successful[0].id(), successful[1].id()],
                    [successful_note_2.id(), successful_note_1.id()]
                );
            }
    );
    Ok(())
}
