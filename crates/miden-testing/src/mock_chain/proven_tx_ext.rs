use miden_objects::{
    account::delta::AccountUpdateDetails,
    transaction::{ExecutedTransaction, ProvenTransaction, ProvenTransactionBuilder},
    vm::ExecutionProof,
};
use winterfell::Proof;

/// Extension trait to convert an [`ExecutedTransaction`] into a [`ProvenTransaction`] with a dummy
/// proof for testing purposes.
pub trait ProvenTransactionExt {
    /// Converts the transaction into a proven transaction with a dummy proof.
    fn from_executed_transaction_mocked(executed_tx: ExecutedTransaction) -> ProvenTransaction;
}

impl ProvenTransactionExt for ProvenTransaction {
    fn from_executed_transaction_mocked(executed_tx: ExecutedTransaction) -> ProvenTransaction {
        let block_reference = executed_tx.block_header();
        let account_delta = executed_tx.account_delta().clone();
        let initial_account = executed_tx.initial_account().clone();

        let account_update_details = if initial_account.is_onchain() {
            if initial_account.is_new() {
                let mut account = initial_account;
                account.apply_delta(&account_delta).expect("account delta should be appliable");

                AccountUpdateDetails::New(account)
            } else {
                AccountUpdateDetails::Delta(account_delta)
            }
        } else {
            AccountUpdateDetails::Private
        };

        ProvenTransactionBuilder::new(
            executed_tx.account_id(),
            executed_tx.initial_account().init_commitment(),
            executed_tx.final_account().commitment(),
            block_reference.block_num(),
            block_reference.commitment(),
            executed_tx.expiration_block_num(),
            ExecutionProof::new(Proof::new_dummy(), Default::default()),
        )
        .add_input_notes(executed_tx.input_notes())
        .add_output_notes(executed_tx.output_notes().iter().cloned())
        .account_update_details(account_update_details)
        .build()
        .unwrap()
    }
}
