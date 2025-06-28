use crate::assert_execution_error;
use miden_lib::{errors::MasmError, transaction::TransactionKernel};
use miden_objects::{
    account::Account,
    testing::{
        account_component::{ConditionalAuthComponent, ERR_WRONG_ARGS_MSG},
        account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    },
    transaction::AuthArguments,
};
use miden_tx::TransactionExecutorError;

use super::{Felt, ONE};
use crate::TransactionContextBuilder;

pub const ERR_WRONG_ARGS: MasmError = MasmError::from_static_str(ERR_WRONG_ARGS_MSG);

#[test]
fn test_auth_procedure_args() {
    let auth_component =
        ConditionalAuthComponent::from_assembler(TransactionKernel::testing_assembler()).unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component.into(),
        TransactionKernel::testing_assembler(),
    );

    let auth_arguments = [
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
        Felt::new(96),
        ONE, // incr_nonce = true
    ];

    let tx_context = TransactionContextBuilder::new(account)
        .auth_arguments(AuthArguments::new(&auth_arguments))
        .build();

    let executed_transaction = tx_context.execute();

    assert!(
        executed_transaction.is_ok(),
        "Transaction execution failed {:?}",
        executed_transaction,
    );
}

#[test]
fn test_auth_procedure_args_wrong_inputs() {
    let auth_component =
        ConditionalAuthComponent::from_assembler(TransactionKernel::testing_assembler()).unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component.into(),
        TransactionKernel::testing_assembler(),
    );

    // The auth script expects [99, 98, 97, 96, nonce_increment_flag]
    let auth_arguments = [
        Felt::new(103),
        Felt::new(102),
        Felt::new(101),
        Felt::new(100),
        ONE, // incr_nonce = true
    ];

    let tx_context = TransactionContextBuilder::new(account)
        .auth_arguments(AuthArguments::new(&auth_arguments))
        .build();

    let executed_transaction = tx_context.execute();

    assert!(executed_transaction.is_err());

    let err = executed_transaction.unwrap_err();

    let TransactionExecutorError::TransactionProgramExecutionFailed(err) = err else {
        panic!("unexpected error")
    };

    assert_execution_error!(Err::<(), _>(err), ERR_WRONG_ARGS);
}
