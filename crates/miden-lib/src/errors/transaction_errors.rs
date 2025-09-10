use thiserror::Error;

// TRANSACTION EVENT PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionEventError {
    #[error("event id {0} is not a valid transaction event")]
    InvalidTransactionEvent(u32),
    #[error("event id {0} is not a transaction kernel event")]
    NotTransactionEvent(u32),
    #[error("event id {0} can only be emitted from the root context")]
    NotRootContext(u32),
}

// TRANSACTION TRACE PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionTraceParsingError {
    #[error("trace id {0} is an unknown transaction kernel trace")]
    UnknownTransactionTrace(u32),
}
