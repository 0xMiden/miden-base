use anyhow::Context;
use miden_lib::account::wallets::BasicWallet;
use miden_lib::errors::MasmError;
use miden_lib::errors::note_script_errors::ERR_AUTH_PROCEDURE_CALLED_FROM_WRONG_CONTEXT;
use miden_lib::transaction::TransactionKernel;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{Account, AccountBuilder};
use miden_objects::testing::account_component::{ConditionalAuthComponent, ERR_WRONG_ARGS_MSG};
use miden_objects::testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE;
use miden_tx::TransactionExecutorError;

use super::{Felt, ONE};
use crate::{Auth, TransactionContextBuilder, assert_execution_error};

pub const ERR_WRONG_ARGS: MasmError = MasmError::from_static_str(ERR_WRONG_ARGS_MSG);

/// Tests that authentication arguments are correctly passed to the auth procedure.
///
/// This test creates an account with a conditional auth component that expects specific
/// auth arguments [97, 98, 99] to not error out. When the correct arguments are provided,
/// the nonce is incremented (because of `incr_nonce_flag`).
#[test]
fn test_auth_procedure_args() -> anyhow::Result<()> {
    let auth_component = ConditionalAuthComponent::new(TransactionKernel::testing_assembler())?;
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component,
        TransactionKernel::testing_assembler(),
    );

    let auth_args = [
        ONE, // incr_nonce = true
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
    ];

    let tx_context = TransactionContextBuilder::new(account).auth_args(auth_args.into()).build()?;

    tx_context.execute().context("failed to execute transaction")?;

    Ok(())
}

/// Tests that incorrect authentication procedure arguments cause transaction execution to fail.
///
/// This test creates an account with a conditional auth component that expects specific
/// auth arguments [97, 98, 99, incr_nonce_flag]. When incorrect arguments are provided
/// (in this case [101, 102, 103]), the transaction should fail with an appropriate error message.
#[test]
fn test_auth_procedure_args_wrong_inputs() -> anyhow::Result<()> {
    let auth_component = ConditionalAuthComponent::new(TransactionKernel::testing_assembler())?;
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component,
        TransactionKernel::testing_assembler(),
    );

    // The auth script expects [99, 98, 97, nonce_increment_flag]
    let auth_args = [
        ONE, // incr_nonce = true
        Felt::new(103),
        Felt::new(102),
        Felt::new(101),
    ];

    let tx_context = TransactionContextBuilder::new(account).auth_args(auth_args.into()).build()?;

    let execution_result = tx_context.execute();

    let TransactionExecutorError::TransactionProgramExecutionFailed(err) =
        execution_result.unwrap_err()
    else {
        panic!("unexpected error type")
    };

    assert_execution_error!(Err::<(), _>(err), ERR_WRONG_ARGS);

    Ok(())
}

/// Tests that attempting to call the auth procedure manually from user code fails.
#[test]
fn test_auth_procedure_called_from_wrong_context() -> anyhow::Result<()> {
    let (auth_component, _) = Auth::IncrNonce.build_component();

    let account = AccountBuilder::new([42; 32])
        .with_auth_component(auth_component.clone())
        .with_component(BasicWallet)
        .build_existing()?;

    // Create a transaction script that calls the auth procedure
    let tx_script_source = "
        begin
            call.::auth__basic
        end
    ";

    let tx_script = ScriptBuilder::default()
        .with_dynamically_linked_library(auth_component.library())?
        .compile_tx_script(tx_script_source)?;

    let tx_context = TransactionContextBuilder::new(account).tx_script(tx_script).build()?;

    let execution_result = tx_context.execute();

    let TransactionExecutorError::TransactionProgramExecutionFailed(err) =
        execution_result.unwrap_err()
    else {
        panic!("unexpected error type")
    };

    assert_execution_error!(Err::<(), _>(err), ERR_AUTH_PROCEDURE_CALLED_FROM_WRONG_CONTEXT);

    Ok(())
}
