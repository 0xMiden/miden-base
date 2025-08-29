use miden_objects::transaction::{ExecutedTransaction, ProvenTransaction};
use miden_tx::LocalTransactionProver;

/// Extension trait to convert an [`ExecutedTransaction`] into a [`ProvenTransaction`] with a dummy
/// proof for testing purposes.
pub trait ProvenTransactionExt {
    /// Converts the transaction into a proven transaction with a dummy proof.
    fn from_executed_transaction_mocked(executed_tx: ExecutedTransaction) -> ProvenTransaction;
}

impl ProvenTransactionExt for ProvenTransaction {
    fn from_executed_transaction_mocked(executed_tx: ExecutedTransaction) -> ProvenTransaction {
        LocalTransactionProver::default().prove_dummy(executed_tx).unwrap()
    }
}
