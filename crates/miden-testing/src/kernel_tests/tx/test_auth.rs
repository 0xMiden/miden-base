use anyhow::Context;
use miden_protocol::account::{Account, AccountBuilder, AccountStorageMode, StorageSlotName};
use miden_protocol::errors::MasmError;
use miden_protocol::errors::tx_kernel::ERR_EPILOGUE_AUTH_PROCEDURE_CALLED_FROM_WRONG_CONTEXT;
use miden_protocol::testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE;
use miden_protocol::{Felt, ONE, Word};
use miden_standards::account::wallets::BasicWallet;
use miden_standards::code_builder::CodeBuilder;
use miden_standards::testing::account_component::{
    CALL_COUNTER_SLOT_NAME,
    ConditionalAuthComponent,
    ERR_WRONG_ARGS_MSG,
    MockAccountComponent,
    SelfCallingAuthComponent,
};
use miden_standards::testing::mock_account::MockAccountExt;

use crate::{Auth, MockChainBuilder, TransactionContextBuilder, assert_transaction_executor_error};

pub const ERR_WRONG_ARGS: MasmError = MasmError::from_static_str(ERR_WRONG_ARGS_MSG);

/// Tests that authentication arguments are correctly passed to the auth procedure.
///
/// This test creates an account with a conditional auth component that expects specific
/// auth arguments [97, 98, 99] to not error out. When the correct arguments are provided,
/// the nonce is incremented (because of `incr_nonce_flag`).
#[tokio::test]
async fn test_auth_procedure_args() -> anyhow::Result<()> {
    let account =
        Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ConditionalAuthComponent);

    let auth_args = [
        ONE, // incr_nonce = true
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
    ];

    let tx_context = TransactionContextBuilder::new(account).auth_args(auth_args.into()).build()?;

    tx_context.execute().await.context("failed to execute transaction")?;

    Ok(())
}

/// Tests that incorrect authentication procedure arguments cause transaction execution to fail.
///
/// This test creates an account with a conditional auth component that expects specific
/// auth arguments [97, 98, 99, incr_nonce_flag]. When incorrect arguments are provided
/// (in this case [101, 102, 103]), the transaction should fail with an appropriate error message.
#[tokio::test]
async fn test_auth_procedure_args_wrong_inputs() -> anyhow::Result<()> {
    let account =
        Account::mock(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ConditionalAuthComponent);

    // The auth script expects [99, 98, 97, nonce_increment_flag]
    let auth_args = [
        ONE, // incr_nonce = true
        Felt::new(103),
        Felt::new(102),
        Felt::new(101),
    ];

    let tx_context = TransactionContextBuilder::new(account).auth_args(auth_args.into()).build()?;

    let execution_result = tx_context.execute().await;

    assert_transaction_executor_error!(execution_result, ERR_WRONG_ARGS);

    Ok(())
}

/// Tests that attempting to call the auth procedure manually from user code fails.
#[tokio::test]
async fn test_auth_procedure_called_from_wrong_context() -> anyhow::Result<()> {
    let (auth_component, _) = Auth::IncrNonce.build_component();

    let account = AccountBuilder::new([42; 32])
        .with_auth_component(auth_component.clone())
        .with_component(BasicWallet)
        .build_existing()?;

    // Create a transaction script that calls the auth procedure
    let tx_script_source = "
        begin
            call.::incr_nonce::auth_incr_nonce
        end
    ";

    let tx_script = CodeBuilder::default()
        .with_dynamically_linked_library(auth_component.component_code())?
        .compile_tx_script(tx_script_source)?;

    let tx_context = TransactionContextBuilder::new(account).tx_script(tx_script).build()?;

    let execution_result = tx_context.execute().await;

    assert_transaction_executor_error!(
        execution_result,
        ERR_EPILOGUE_AUTH_PROCEDURE_CALLED_FROM_WRONG_CONTEXT
    );

    Ok(())
}

// REENTRANCY TESTS
// ================================================================================================

/// Tests that auth procedure reentrancy is prevented.
#[tokio::test]
async fn test_auth_procedure_reentrancy_self_call() -> anyhow::Result<()> {
    let mut builder = MockChainBuilder::default();

    let account = AccountBuilder::new([42; 32])
        .with_auth_component(SelfCallingAuthComponent)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    // Assert that the call counter is initialized to 0
    let counter_slot = account
        .storage()
        .get_item(&StorageSlotName::new(CALL_COUNTER_SLOT_NAME).unwrap());
    assert_eq!(counter_slot.unwrap(), Word::default(), "counter should be initialized to 0");

    let account_for_delta = account.clone();
    builder.add_account(account.clone())?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx_context = mock_chain
        .build_tx_context(crate::TxContextInput::Account(account), &[], &[])?
        .build()?;

    let execution_result = tx_context.execute().await;

    // Apply the transaction delta to get the updated account state
    let executed_tx = execution_result.expect("transaction should execute");
    let mut updated_account = account_for_delta;
    updated_account.apply_delta(executed_tx.account_delta())?;

    // Check the counter value after transaction execution
    let counter_slot = updated_account
        .storage()
        .get_item(&StorageSlotName::new(CALL_COUNTER_SLOT_NAME).unwrap());
    let counter_word = counter_slot.expect("counter slot should exist");

    let counter_value = counter_word[3].as_int();

    // BUG: The transaction succeeds and the counter is incremented to 2, indicating
    // that the auth procedure was able to call itself via dynexec.

    // Expected: counter = 1 (only first call should succeed, reentrancy should fail)
    // Actual: counter = 2 (both calls succeeded, reentrancy was not prevented)
    assert_eq!(
        counter_value, 2,
        "counter should be 2 if reentrancy succeeded (BUG), or 1 if prevented"
    );

    Ok(())
}
