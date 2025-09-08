use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::asset::Asset;
use miden_objects::transaction::{
    OutputNote,
    ProvenTransaction,
    ProvenTransactionBuilder,
    TransactionWitness,
};
pub use miden_prover::ProvingOptions;
use miden_prover::prove;

use super::TransactionProverError;
use crate::host::{AccountProcedureIndexMap, ScriptMastForestStore};

mod prover_host;
pub use prover_host::TransactionProverHost;

mod mast_store;
pub use mast_store::TransactionMastStore;

// LOCAL TRANSACTION PROVER
// ------------------------------------------------------------------------------------------------

/// Local Transaction prover is a stateless component which is responsible for proving transactions.
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

impl LocalTransactionProver {
    pub fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let mast_store = self.mast_store.clone();
        let proof_options = self.proof_options.clone();

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
        for account_inputs in tx_args.foreign_account_inputs() {
            self.mast_store.load_account_code(account_inputs.code());
        }

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
                mast_store.as_ref(),
                script_mast_store,
                acct_procedure_index_map,
            )
        };

        let advice_inputs = advice_inputs.into_advice_inputs();

        let (stack_outputs, proof) = prove(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs.clone(),
            &mut host,
            proof_options,
        )
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        // Extract transaction outputs and process transaction data.
        // Note that the account delta does not contain the removed transaction fee, so it is the
        // "pre-fee" delta of the transaction.
        let (pre_fee_account_delta, output_notes, _tx_progress) = host.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        // erase private note information (convert private full notes to just headers)
        let output_notes: Vec<_> = tx_outputs.output_notes.iter().map(OutputNote::shrink).collect();

        // Compute the commitment of the pre-fee delta, which goes into the proven transaction,
        // since it is the output of the transaction and so is needed for proof verification.
        let pre_fee_delta_commitment = pre_fee_account_delta.to_commitment();

        let builder = ProvenTransactionBuilder::new(
            account.id(),
            account.init_commitment(),
            tx_outputs.account.commitment(),
            pre_fee_delta_commitment,
            ref_block_num,
            ref_block_commitment,
            tx_outputs.fee,
            tx_outputs.expiration_block_num,
            proof,
        )
        .add_input_notes(input_notes)
        .add_output_notes(output_notes);

        // The full transaction delta is the pre fee delta with the fee asset removed.
        let mut post_fee_account_delta = pre_fee_account_delta;
        post_fee_account_delta
            .vault_mut()
            .remove_asset(Asset::from(tx_outputs.fee))
            .map_err(TransactionProverError::RemoveFeeAssetFromDelta)?;

        // If the account is on-chain, add the update details.
        let builder = match account.is_onchain() {
            true => {
                let account_update_details = if account.is_new() {
                    let mut account = account.clone();
                    account
                        .apply_delta(&post_fee_account_delta)
                        .map_err(TransactionProverError::AccountDeltaApplyFailed)?;

                    AccountUpdateDetails::New(account)
                } else {
                    AccountUpdateDetails::Delta(post_fee_account_delta)
                };

                builder.account_update_details(account_update_details)
            },
            false => builder,
        };

        builder.build().map_err(TransactionProverError::ProvenTransactionBuildFailed)
    }
}
