extern crate alloc;

mod scripts;
mod wallet;

// HELPER FUNCTIONS
// ================================================================================================

#[macro_export]
macro_rules! assert_transaction_executor_error {
    ($execution_result:expr, $expected_err:expr) => {
        match $execution_result {
            Err(miden_tx::TransactionExecutorError::TransactionProgramExecutionFailed(
                vm_processor::ExecutionError::FailedAssertion {
                    label: _,
                    source_file: _,
                    clk: _,
                    err_code,
                    err_msg,
                },
            )) => {
                if let Some(ref msg) = err_msg {
                  assert_eq!(msg.as_ref(), $expected_err.message(), "error messages did not match");
                }

                assert_eq!(
                  err_code, $expected_err.code(),
                  "Execution failed on assertion with an unexpected error (Actual code: {}, msg: {}, Expected: {}).",
                  err_code, err_msg.as_ref().map(|string| string.as_ref()).unwrap_or("<no message>"), $expected_err);
            },
            Ok(_) => panic!("Execution was unexpectedly successful"),
            Err(err) => panic!("Execution error was not as expected: {err}"),
        }
    };
}
