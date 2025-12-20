/// The errors from the MASM code of the transaction kernel.
#[cfg(any(feature = "testing", test))]
#[rustfmt::skip]
pub mod tx_kernel;

/// The errors from the MASM code of the Miden protocol library.
#[cfg(any(feature = "testing", test))]
#[rustfmt::skip]
pub mod protocol;

/// The errors from the MASM code of the Miden standards.
#[cfg(any(feature = "testing", test))]
#[rustfmt::skip]
pub mod standards;

mod masm_error;
pub use masm_error::MasmError;

mod code_builder_errors;
pub use code_builder_errors::CodeBuilderError;

mod transaction_errors;
pub use transaction_errors::{TransactionEventError, TransactionTraceParsingError};
