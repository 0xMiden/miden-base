#[cfg(feature = "async")]
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::asset::Asset;
use miden_objects::transaction::{
    OutputNote,
    ProvenTransaction,
    ProvenTransactionBuilder,
    TransactionWitness,
};
pub use miden_prover::ProvingOptions;
use miden_prover::prove;
use winter_maybe_async::*;

use super::TransactionProverError;
use crate::host::{AccountProcedureIndexMap, ScriptMastForestStore};

mod prover_host;
pub use prover_host::TransactionProverHost;

mod mast_store;
pub use mast_store::TransactionMastStore;

// TRANSACTION PROVER TRAIT
// ================================================================================================

/// The [TransactionProver] trait defines the interface that transaction witness objects use to
/// prove transactions and generate a [ProvenTransaction].
#[maybe_async_trait]
pub trait TransactionProver {
    /// Proves the provided transaction and returns a [ProvenTransaction].
    ///
    /// # Errors
    /// - If the input note data in the transaction witness is corrupt.
    /// - If the transaction program cannot be proven.
    /// - If the transaction result is corrupt.
    #[maybe_async]
    fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError>;
}

// LOCAL TRANSACTION PROVER
// ------------------------------------------------------------------------------------------------

/// Local Transaction prover is a stateless component which is responsible for proving transactions.
///
/// Local Transaction Prover implements the [TransactionProver] trait.
pub struct LocalTransactionProver {
    mast_store: Arc<TransactionMastStore>,
    proof_options: ProvingOptions,
}

impl LocalTransactionProver {
    /// Creates a new [LocalTransactionProver] instance.
    pub fn new(proof_options: ProvingOptions) -> Self {
        Self {
            mast_store: Arc::new(TransactionMastStore::new()),
            proof_options,
        }
    }
}

impl Default for LocalTransactionProver {
    fn default() -> Self {
        Self {
            mast_store: Arc::new(TransactionMastStore::new()),
            proof_options: Default::default(),
        }
    }
}

#[maybe_async_trait]
impl TransactionProver for LocalTransactionProver {
    #[maybe_async]
    fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let TransactionWitness { tx_inputs, tx_args, advice_witness } = tx_witness;

        let account = tx_inputs.account();
        let input_notes = tx_inputs.input_notes();
        let ref_block_num = tx_inputs.block_header().block_num();
        let ref_block_commitment = tx_inputs.block_header().commitment();

        // execute and prove
        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_witness))
                .map_err(TransactionProverError::ConflictingAdviceMapEntry)?;

        // load the store with account/note/tx_script MASTs
        self.mast_store.load_account_code(account.code());

        let script_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            input_notes.iter().map(|n| n.note().script()),
        );

        let mut host = {
            let acct_procedure_index_map = AccountProcedureIndexMap::from_transaction_params(
                &tx_inputs,
                &tx_args,
                &advice_inputs,
            )
            .map_err(TransactionProverError::TransactionHostCreationFailed)?;

            TransactionProverHost::new(
                &account.into(),
                input_notes.clone(),
                self.mast_store.as_ref(),
                script_mast_store,
                acct_procedure_index_map,
            )
        };

        let advice_inputs = advice_inputs.into_advice_inputs();

        // For the prover, we assume that the transaction witness was successfully executed and so
        // there is no need to provide the actual source manager, as it is only used to improve
        // error quality. So we simply pass an empty one.
        let source_manager = Arc::new(DefaultSourceManager::default());
        let (stack_outputs, proof) = maybe_await!(prove(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs.clone(),
            &mut host,
            self.proof_options.clone(),
            source_manager
        ))
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        // extract transaction outputs and process transaction data
        let (mut account_delta, output_notes, _tx_progress) = host.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        // erase private note information (convert private full notes to just headers)
        let output_notes: Vec<_> = tx_outputs.output_notes.iter().map(OutputNote::shrink).collect();

        // Because the fee asset is removed from the vault after the commitment is computed in the
        // kernel, we have to *add* it to the delta before compute the commitment against which the
        // transaction is proven.
        // Note that the fee asset is a transaction output and so is part of the proof. The delta
        // without the added fee asset can still be validated by repeating this process.
        account_delta
            .vault_mut()
            .add_asset(Asset::from(tx_outputs.fee))
            .map_err(TransactionProverError::FailedToMutateAccountDeltaWithFee)?;

        let account_delta_commitment = account_delta.to_commitment();

        // Now that we have computed the commitment of the delta with the fee, we revert the above
        // changes to get back the actual account delta of the transaction.
        account_delta
            .vault_mut()
            .remove_asset(Asset::from(tx_outputs.fee))
            .map_err(TransactionProverError::FailedToMutateAccountDeltaWithFee)?;

        let builder = ProvenTransactionBuilder::new(
            account.id(),
            account.init_commitment(),
            tx_outputs.account.commitment(),
            account_delta_commitment,
            ref_block_num,
            ref_block_commitment,
            tx_outputs.fee,
            tx_outputs.expiration_block_num,
            proof,
        )
        .add_input_notes(input_notes)
        .add_output_notes(output_notes);

        // If the account is on-chain, add the update details.
        let builder = match account.is_onchain() {
            true => {
                let account_update_details = if account.is_new() {
                    let mut account = account.clone();
                    account
                        .apply_delta(&account_delta)
                        .map_err(TransactionProverError::AccountDeltaApplyFailed)?;

                    AccountUpdateDetails::New(account)
                } else {
                    AccountUpdateDetails::Delta(account_delta)
                };

                builder.account_update_details(account_update_details)
            },
            false => builder,
        };

        builder.build().map_err(TransactionProverError::ProvenTransactionBuildFailed)
    }
}
