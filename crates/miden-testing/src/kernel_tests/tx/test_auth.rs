use miden_lib::{errors::MasmError, transaction::TransactionKernel};
use miden_objects::{
    account::Account,
    testing::{
        account_component::{ConditionalAuthComponent, ERR_WRONG_ARGS_MSG},
        account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    },
};
use miden_tx::TransactionExecutorError;

use super::{Felt, ONE};
use crate::{TransactionContextBuilder, assert_execution_error};

pub const ERR_WRONG_ARGS: MasmError = MasmError::from_static_str(ERR_WRONG_ARGS_MSG);

#[test]
fn test_auth_procedure_args() {
    let auth_component =
        ConditionalAuthComponent::new(TransactionKernel::testing_assembler()).unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component,
        TransactionKernel::testing_assembler(),
    );

    let auth_arg = [
        ONE, // incr_nonce = true
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
    ];

    let tx_context = TransactionContextBuilder::new(account)
        .auth_arg(auth_arg.into())
        .build()
        .unwrap();

    let executed_transaction = tx_context.execute();

    assert!(
        executed_transaction.is_ok(),
        "Transaction execution failed {executed_transaction:?}",
    );
}

#[test]
fn test_auth_procedure_args_wrong_inputs() {
    let auth_component =
        ConditionalAuthComponent::new(TransactionKernel::testing_assembler()).unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component,
        TransactionKernel::testing_assembler(),
    );

    // The auth script expects [99, 98, 97, nonce_increment_flag]
    let auth_arg = [
        ONE, // incr_nonce = true
        Felt::new(103),
        Felt::new(102),
        Felt::new(101),
    ];

    let tx_context = TransactionContextBuilder::new(account)
        .auth_arg(auth_arg.into())
        .build()
        .unwrap();

    let executed_transaction = tx_context.execute();

    assert!(executed_transaction.is_err());

    let err = executed_transaction.unwrap_err();

    let TransactionExecutorError::TransactionProgramExecutionFailed(err) = err else {
        panic!("unexpected error")
    };

    assert_execution_error!(Err::<(), _>(err), ERR_WRONG_ARGS);
}
