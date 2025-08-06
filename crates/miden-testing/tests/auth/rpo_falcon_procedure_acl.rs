use assert_matches::assert_matches;
use miden_lib::transaction::{TransactionKernel, TransactionKernelError};
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorage,
    AccountStorageMode,
    AccountType,
};
use miden_objects::testing::account_component::AccountMockComponent;
use miden_objects::testing::account_id::ACCOUNT_ID_SENDER;
use miden_objects::testing::note::NoteBuilder;
use miden_objects::transaction::OutputNote;
use miden_objects::{Felt, FieldElement, Word};
use miden_testing::{Auth, MockChain};
use miden_tx::TransactionExecutorError;
use vm_processor::ExecutionError;

// CONSTANTS
// ================================================================================================

const TX_SCRIPT_NO_TRIGGER: &str = r#"
    use.test::account
    begin
        call.account::account_procedure_1
        drop
    end
    "#;

// HELPER FUNCTIONS
// ================================================================================================

/// Sets up the basic components needed for RPO Falcon procedure ACL tests.
/// Returns (account, mock_chain, note).
fn setup_rpo_falcon_procedure_acl_test(
    allow_unauthorized_output_notes: bool,
    allow_unauthorized_input_notes: bool,
) -> anyhow::Result<(miden_objects::account::Account, MockChain, miden_objects::note::Note)> {
    let assembler = TransactionKernel::assembler();

    let component: AccountComponent = AccountMockComponent::new_with_slots(
        assembler.clone(),
        AccountStorage::mock_storage_slots(),
    )
    .expect("failed to create mock component")
    .into();

    let get_item_proc_root = component
        .get_procedure_root_by_name("test::account::get_item")
        .expect("get_item procedure should exist");
    let set_item_proc_root = component
        .get_procedure_root_by_name("test::account::set_item")
        .expect("set_item procedure should exist");
    let auth_trigger_procedures = vec![get_item_proc_root, set_item_proc_root];

    let (auth_component, _authenticator) = Auth::ProcedureAcl {
        auth_trigger_procedures: auth_trigger_procedures.clone(),
        allow_unauthorized_output_notes,
        allow_unauthorized_input_notes,
    }
    .build_component();

    let account = AccountBuilder::new([0; 32])
        .with_auth_component(auth_component)
        .with_component(component)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;
    let mock_chain = builder.build()?;

    // Create a mock note to consume (needed to make the transaction non-empty)
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;
    let note = NoteBuilder::new(sender_id, &mut rand::rng())
        .build(&assembler)
        .expect("failed to create mock note");

    Ok((account, mock_chain, note))
}

#[test]
fn test_rpo_falcon_procedure_acl() -> anyhow::Result<()> {
    let (account, mut mock_chain, note) = setup_rpo_falcon_procedure_acl_test(false, true)?;

    // We need to get the authenticator separately for this test
    let assembler = TransactionKernel::assembler();
    let component: AccountComponent = AccountMockComponent::new_with_slots(
        assembler.clone(),
        AccountStorage::mock_storage_slots(),
    )
    .expect("failed to create mock component")
    .into();

    let get_item_proc_root = component
        .get_procedure_root_by_name("test::account::get_item")
        .expect("get_item procedure should exist");
    let set_item_proc_root = component
        .get_procedure_root_by_name("test::account::set_item")
        .expect("set_item procedure should exist");
    let auth_trigger_procedures = vec![get_item_proc_root, set_item_proc_root];

    let (_, authenticator) = Auth::ProcedureAcl {
        auth_trigger_procedures: auth_trigger_procedures.clone(),
        allow_unauthorized_output_notes: false,
        allow_unauthorized_input_notes: true,
    }
    .build_component();

    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block()?;

    let tx_script_with_trigger_1 = r#"
        use.test::account

        begin
            push.0
            call.account::get_item
            dropw
        end
        "#;

    let tx_script_with_trigger_2 = r#"
        use.test::account

        begin
            push.1.2.3.4 push.0
            call.account::set_item
            dropw dropw
        end
        "#;

    let tx_script_trigger_1 =
        ScriptBuilder::with_mock_account_library()?.compile_tx_script(tx_script_with_trigger_1)?;

    let tx_script_trigger_2 =
        ScriptBuilder::with_mock_account_library()?.compile_tx_script(tx_script_with_trigger_2)?;

    let tx_script_no_trigger =
        ScriptBuilder::with_mock_account_library()?.compile_tx_script(TX_SCRIPT_NO_TRIGGER)?;

    // Test 1: Transaction WITH authenticator calling trigger procedure 1 (should succeed)
    let tx_context_with_auth_1 = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(authenticator.clone())
        .tx_script(tx_script_trigger_1.clone())
        .build()?;

    tx_context_with_auth_1
        .execute_blocking()
        .expect("trigger 1 with auth should succeed");

    // Test 2: Transaction WITH authenticator calling trigger procedure 2 (should succeed)
    let tx_context_with_auth_2 = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(authenticator)
        .tx_script(tx_script_trigger_2)
        .build()?;

    tx_context_with_auth_2
        .execute_blocking()
        .expect("trigger 2 with auth should succeed");

    // Test 3: Transaction WITHOUT authenticator calling trigger procedure (should fail)
    let tx_context_no_auth = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_trigger_1)
        .build()?;

    let executed_tx_no_auth = tx_context_no_auth.execute_blocking();

    assert_matches!(executed_tx_no_auth, Err(TransactionExecutorError::TransactionProgramExecutionFailed(
        execution_error
    )) => {
        assert_matches!(execution_error, ExecutionError::EventError { error, .. } => {
            let kernel_error = error.downcast_ref::<TransactionKernelError>().unwrap();
            assert_matches!(kernel_error, TransactionKernelError::MissingAuthenticator);
        })
    });

    // Test 4: Transaction WITHOUT authenticator calling non-trigger procedure (should succeed)
    let tx_context_no_trigger = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_no_trigger)
        .build()?;

    let executed = tx_context_no_trigger
        .execute_blocking()
        .expect("no trigger, no auth should succeed");
    assert_eq!(
        executed.account_delta().nonce_delta(),
        Felt::ZERO,
        "no auth but should still trigger nonce increment"
    );

    Ok(())
}

#[test]
fn test_rpo_falcon_procedure_acl_with_allow_unauthorized_output_notes() -> anyhow::Result<()> {
    let (account, mock_chain, note) = setup_rpo_falcon_procedure_acl_test(true, true)?;

    // Verify the storage layout includes both authorization flags
    let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
    // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
    // allow_unauthorized_input_notes, 0] With 2 procedures,
    // allow_unauthorized_output_notes=true, and allow_unauthorized_input_notes=true, this should be
    // [2, 1, 1, 0]
    assert_eq!(slot_1, Word::from([2u32, 1, 1, 0]));

    let tx_script_no_trigger =
        ScriptBuilder::with_mock_account_library()?.compile_tx_script(TX_SCRIPT_NO_TRIGGER)?;

    // Test: Transaction WITHOUT authenticator calling non-trigger procedure (should succeed)
    // This tests that when allow_unauthorized_output_notes=true, transactions without
    // authenticators can still succeed even if they create output notes
    let tx_context_no_trigger = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_no_trigger)
        .build()?;

    let executed = tx_context_no_trigger
        .execute_blocking()
        .expect("no trigger, no auth should succeed");
    assert_eq!(
        executed.account_delta().nonce_delta(),
        Felt::ZERO,
        "no auth but should still trigger nonce increment"
    );

    Ok(())
}

#[test]
fn test_rpo_falcon_procedure_acl_with_disallow_unauthorized_input_notes() -> anyhow::Result<()> {
    let (account, mock_chain, note) = setup_rpo_falcon_procedure_acl_test(true, false)?;

    // Verify the storage layout includes both flags
    let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
    // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
    // allow_unauthorized_input_notes, 0] With 2 procedures,
    // allow_unauthorized_output_notes=true, and allow_unauthorized_input_notes=false, this should
    // be [2, 1, 0, 0]
    assert_eq!(slot_1, Word::from([2u32, 1, 0, 0]));

    let tx_script_no_trigger =
        ScriptBuilder::with_mock_account_library()?.compile_tx_script(TX_SCRIPT_NO_TRIGGER)?;

    // Test: Transaction WITHOUT authenticator calling non-trigger procedure but consuming input
    // notes This should FAIL because allow_unauthorized_input_notes=false and we're consuming
    // input notes
    let tx_context_no_auth = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_no_trigger)
        .build()?;

    let executed_tx_no_auth = tx_context_no_auth.execute_blocking();

    // This should fail with MissingAuthenticator error because input notes are being consumed
    // and allow_unauthorized_input_notes is false
    assert_matches!(executed_tx_no_auth, Err(TransactionExecutorError::TransactionProgramExecutionFailed(
        execution_error
    )) => {
        assert_matches!(execution_error, ExecutionError::EventError { error, .. } => {
            let kernel_error = error.downcast_ref::<TransactionKernelError>().unwrap();
            assert_matches!(kernel_error, TransactionKernelError::MissingAuthenticator);
        })
    });

    Ok(())
}
