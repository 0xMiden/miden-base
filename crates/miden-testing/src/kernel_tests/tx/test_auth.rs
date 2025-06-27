use crate::assert_execution_error;
use alloc::string::String;
use assembly::Library;
use miden_lib::{errors::MasmError, transaction::TransactionKernel};
use miden_objects::{
    AccountError,
    account::{Account, AccountComponent},
    testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    transaction::AuthArguments,
};
use miden_tx::TransactionExecutorError;
use std::sync::LazyLock;

use super::{Felt, ONE};
use crate::TransactionContextBuilder;

pub const ERR_WRONG_ARGS_MSG: &str = "auth procedure args are incorrect";
pub const ERR_WRONG_ARGS: MasmError = MasmError::from_static_str(ERR_WRONG_ARGS_MSG);

static AUTH_COMPONENT_CODE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
    use.miden::account

    const.WRONG_ARGS="{}"

    export.auth
        # OS => [AUTH_ARGS_KEY]
        # AS => []

        # `AUTH_ARGS_KEY` value, which is located on the stack at the beginning of 
        # the execution, is the advice map key which allows to obtain auth procedure args 
        # which were specified during the `AuthArguments` creation.

        # move the auth args from advice map to the advice stack
        adv.push_mapval
        # OS => [AUTH_ARGS_KEY]
        # AS => [96, 97, 98, 99]

        # drop the args commitment
        dropw
        # OS => []
        # AS => [96, 97, 98, 99]

        # Move the auth arguments array from advice stack to the operand stack. It 
        # consists of 4 Felts, so we can use `adv_push.4` instruction to load them all at once
        adv_push.4
        # OS => [96, 97, 98, 99]
        # AS => []

        # If [96, 97, 98, 99] is passed as an argument, all good.
        # Otherwise we error out.
        push.96.97.98.99 eqw assert.err=WRONG_ARGS

        push.1 exec.account::incr_nonce

        dropw dropw dropw dropw
    end
"#,
        ERR_WRONG_ARGS_MSG
    )
});

pub struct AuthComponentWithInputArgs {
    library: Library,
}

impl AuthComponentWithInputArgs {
    pub fn new() -> Result<Self, AccountError> {
        let assembler = TransactionKernel::testing_assembler();

        let library = assembler
            .assemble_library([AUTH_COMPONENT_CODE.as_str()])
            .map_err(AccountError::AccountComponentAssemblyError)?;
        Ok(Self { library })
    }
}

impl From<AuthComponentWithInputArgs> for AccountComponent {
    fn from(mock_component: AuthComponentWithInputArgs) -> Self {
        AccountComponent::new(mock_component.library, vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}

#[test]
fn test_auth_procedure_args() {
    let auth_component = AuthComponentWithInputArgs::new().unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component.into(),
        TransactionKernel::testing_assembler(),
    );

    let auth_arguments = [Felt::new(96), Felt::new(97), Felt::new(98), Felt::new(99)];

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
    let auth_component = AuthComponentWithInputArgs::new().unwrap();
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        ONE,
        auth_component.into(),
        TransactionKernel::testing_assembler(),
    );

    // The auth script expects [96, 97, 98, 99]
    let auth_arguments = [Felt::new(100), Felt::new(101), Felt::new(102), Felt::new(103)];

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
