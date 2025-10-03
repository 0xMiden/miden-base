use miden_core::EventId;
use thiserror::Error;

use crate::transaction::TransactionEvent;

// TRANSACTION EVENT PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionEventError {
    #[error("event id {0} is reserved for system events")]
    ReservedSystemEvent(EventId),
    #[error("event id {0} is not a valid transaction event")]
    InvalidTransactionEvent(EventId, Option<&'static str>),
    #[error("event id {0} is not a transaction kernel event")]
    NotTransactionEvent(EventId, Option<&'static str>),
    #[error("event id {0} can only be emitted from the root context")]
    NotRootContext(TransactionEvent),
}

// TRANSACTION TRACE PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionTraceParsingError {
    #[error("trace id {0} is an unknown transaction kernel trace")]
    UnknownTransactionTrace(u32),
}
