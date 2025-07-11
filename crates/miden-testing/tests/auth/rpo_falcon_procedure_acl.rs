use assert_matches::assert_matches;
use miden_lib::transaction::{TransactionKernel, TransactionKernelError};
use miden_objects::{
    Felt, FieldElement,
    account::{
        AccountBuilder, AccountComponent, AccountId, AccountStorage, AccountStorageMode,
        AccountType,
    },
    testing::{
        account_component::AccountMockComponent, account_id::ACCOUNT_ID_SENDER, note::NoteBuilder,
    },
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain};
use miden_tx::TransactionExecutorError;
use vm_processor::ExecutionError;

#[test]
fn test_rpo_falcon_procedure_acl() -> anyhow::Result<()> {
    let assembler = TransactionKernel::assembler();

    let component: AccountComponent = AccountMockComponent::new_with_slots(
        assembler.clone(),
        AccountStorage::mock_storage_slots(),
    )
    .expect("failed to create mock component")
    .into();

    let mut get_item_proc_root = None;
    let mut set_item_proc_root = None;

    for module in component.library().module_infos() {
        for (_, procedure_info) in module.procedures() {
            match procedure_info.name.as_str() {
                "get_item" => get_item_proc_root = Some(procedure_info.digest),
                "set_item" => set_item_proc_root = Some(procedure_info.digest),
                _ => {},
            }
        }
    }

    let get_item_proc_root = get_item_proc_root.expect("get_item procedure should exist");
    let set_item_proc_root = set_item_proc_root.expect("set_item procedure should exist");
    let auth_trigger_procedures = vec![get_item_proc_root, set_item_proc_root];

    let (auth_component, authenticator) = Auth::ProcedureAcl {
        auth_trigger_procedures: auth_trigger_procedures.clone(),
    }
    .build_component();

    let account = AccountBuilder::new([0; 32])
        .with_auth_component(auth_component)
        .with_component(component)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain = MockChain::new();
    mock_chain.add_pending_account(account.clone());
    mock_chain.prove_next_block()?;

    // Create a mock note to consume (needed to make the transaction non-empty)
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;

    let note = NoteBuilder::new(sender_id, &mut rand::rng())
        .build(&assembler)
        .expect("failed to create mock note");

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

    let tx_script_no_trigger = r#"
        use.test::account

        begin
            call.account::account_procedure_1
            drop
        end
        "#;

    let tx_script_trigger_1 = TransactionScript::compile(
        tx_script_with_trigger_1,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    let tx_script_trigger_2 = TransactionScript::compile(
        tx_script_with_trigger_2,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    let tx_script_no_trigger = TransactionScript::compile(
        tx_script_no_trigger,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    // Test 1: Transaction WITH authenticator calling trigger procedure 1 (should succeed)
    let tx_context_with_auth_1 = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(authenticator.clone())
        .tx_script(tx_script_trigger_1.clone())
        .build()?;

    tx_context_with_auth_1.execute().expect("trigger 1 with auth should succeed");

    // Test 2: Transaction WITH authenticator calling trigger procedure 2 (should succeed)
    let tx_context_with_auth_2 = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(authenticator)
        .tx_script(tx_script_trigger_2)
        .build()?;

    tx_context_with_auth_2.execute().expect("trigger 2 with auth should succeed");

    // Test 3: Transaction WITHOUT authenticator calling trigger procedure (should fail)
    let tx_context_no_auth = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_trigger_1)
        .build()?;

    let executed_tx_no_auth = tx_context_no_auth.execute();

    assert_matches!(executed_tx_no_auth, Err(TransactionExecutorError::TransactionProgramExecutionFailed(
        execution_error
    )) => {
        assert_matches!(execution_error, ExecutionError::EventError { error, .. } => {
            let kernel_error = error.downcast_ref::<TransactionKernelError>().unwrap();
            assert_matches!(kernel_error, TransactionKernelError::FailedSignatureGeneration(msg) => {
                assert_eq!(
                    *msg, "No authenticator assigned to transaction host",
                    "Expected 'No authenticator assigned to transaction host' error, got: {msg}"
                );
            })
        })
    });

    // Test 4: Transaction WITHOUT authenticator calling non-trigger procedure (should succeed)
    let tx_context_no_trigger = mock_chain
        .build_tx_context(account.id(), &[], &[note.clone()])?
        .authenticator(None)
        .tx_script(tx_script_no_trigger)
        .build()?;

    let executed = tx_context_no_trigger.execute().expect("no trigger, no auth should succeed");
    assert_eq!(
        executed.account_delta().nonce_delta(),
        Felt::ONE,
        "no auth but should still trigger nonce increment"
    );

    Ok(())
}
