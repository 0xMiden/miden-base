use assert_matches::assert_matches;
use miden_lib::{
    note::{create_p2id_note, create_p2ide_note},
    transaction::TransactionKernel,
};
use miden_objects::{
    Felt, FieldElement, Word,
    account::{Account, AccountId},
    asset::FungibleAsset,
    note::NoteType,
    testing::{
        account_id::{
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER,
        },
        note::NoteBuilder,
    },
};
use miden_tx::{
    NoteAccountExecution, NoteConsumptionChecker, TransactionExecutor, TransactionExecutorError,
    auth::UnreachableAuth,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use vm_processor::{ExecutionError, crypto::RpoRandomCoin};

use crate::{Auth, MockChain, TransactionContextBuilder, TxContextInput, utils::create_p2any_note};

#[test]
fn check_note_consumability_well_known_notes_success() -> anyhow::Result<()> {
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

    let tx_context = TransactionContextBuilder::with_existing_mock_account()
        .extend_input_notes(vec![p2id_note, p2ide_note])
        .build()?;
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let target_account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context, None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker.check_notes_consumability(
        target_account_id,
        block_ref,
        input_notes,
        tx_args,
        source_manager,
    )?;
    assert_matches!(execution_check_result, NoteAccountExecution::Success);

    Ok(())
}

#[test]
fn check_note_consumability_custom_notes_success() -> anyhow::Result<()> {
    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );
        let input_note =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note])
            .build()?
    };
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context, None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker.check_notes_consumability(
        account_id,
        block_ref,
        input_notes,
        tx_args,
        source_manager,
    )?;
    assert_matches!(execution_check_result, NoteAccountExecution::Success);

    Ok(())
}

#[test]
fn check_note_consumability_failure() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(Auth::BasicAuth)?;
    let mock_chain = builder.build()?;

    let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let failing_note_1 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.0 div end")
    .build(&TransactionKernel::testing_assembler())?;

    let failing_note_2 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.2 drop push.0 div end")
    .build(&TransactionKernel::testing_assembler())?;

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
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor =
        TransactionExecutor::<'_, '_, _, UnreachableAuth>::new(&tx_context, None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker.check_notes_consumability(
        account_id,
        block_ref,
        input_notes,
        tx_args,
        source_manager,
    )?;

    assert_matches!(execution_check_result, NoteAccountExecution::Failure {
        failed_note_id,
        successful_notes,
        error: Some(e)} => {
            assert_eq!(failed_note_id, failing_note_2.id());
            assert_eq!(successful_notes, [successful_note_2.id(),successful_note_1.id()].to_vec());
            assert_matches!(e, TransactionExecutorError::TransactionProgramExecutionFailed(
              ExecutionError::DivideByZero { .. }
            ));
        }
    );
    Ok(())
}
