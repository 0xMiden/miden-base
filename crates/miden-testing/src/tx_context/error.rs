use alloc::string::ToString;
use core::fmt::Display;

use miden_objects::assembly::diagnostics::reporting::PrintDiagnostic;
use miden_processor::ExecutionError;
use thiserror::Error;

#[derive(Debug, Error)]
pub struct ExecError(pub ExecutionError);

impl Display for ExecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&PrintDiagnostic::new(&self.0).to_string())
    }
}

impl From<ExecutionError> for ExecError {
    fn from(err: ExecutionError) -> Self {
        ExecError(err)
    }
}

impl From<ExecError> for ExecutionError {
    fn from(err: ExecError) -> Self {
        err.0
    }
}
