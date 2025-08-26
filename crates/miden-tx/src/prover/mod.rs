use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::account::{Account, AccountDelta};
use miden_objects::block::BlockNumber;
use miden_objects::transaction::{
    ExecutedTransaction,
    InputNote,
    InputNotes,
    OutputNote,
    ProvenTransaction,
    ProvenTransactionBuilder,
    TransactionOutputs,
    TransactionWitness,
};
pub use miden_prover::ProvingOptions;
use miden_prover::{ExecutionProof, Word, prove};

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

    #[cfg(any(feature = "testing", test))]
    pub fn prove_dummy(
        &self,
        executed_tx: ExecutedTransaction,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let (account_delta, tx_outputs, tx_witness, _) = executed_tx.into_parts();

        self.build_proven_transaction(
            tx_witness.tx_inputs.input_notes(),
            tx_outputs,
            account_delta,
            tx_witness.tx_inputs.account(),
            tx_witness.tx_inputs.block_header().block_num(),
            tx_witness.tx_inputs.block_header().commitment(),
            ExecutionProof::new_dummy(),
        )
    }

    fn build_proven_transaction(
        &self,
        input_notes: &InputNotes<InputNote>,
        tx_outputs: TransactionOutputs,
        account_delta: AccountDelta,
        account: &Account,
        ref_block_num: BlockNumber,
        ref_block_commitment: Word,
        proof: ExecutionProof,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let output_notes: Vec<_> = tx_outputs.output_notes.iter().map(OutputNote::shrink).collect();
        let account_delta_commitment: Word = account_delta.to_commitment();

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

        let builder = if account.is_onchain() {
            let details = if account.is_new() {
                let mut account = account.clone();
                account
                    .apply_delta(&account_delta)
                    .map_err(TransactionProverError::AccountDeltaApplyFailed)?;
                AccountUpdateDetails::New(account)
            } else {
                AccountUpdateDetails::Delta(account_delta)
            };
            builder.account_update_details(details)
        } else {
            builder
        };

        builder.build().map_err(TransactionProverError::ProvenTransactionBuildFailed)
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
        let TransactionWitness { tx_inputs, tx_args, advice_witness } = tx_witness;

        let account = tx_inputs.account();
        let input_notes = tx_inputs.input_notes();
        let ref_block_num = tx_inputs.block_header().block_num();
        let ref_block_commitment = tx_inputs.block_header().commitment();

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_witness))
                .map_err(TransactionProverError::ConflictingAdviceMapEntry)?;

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

        let (stack_outputs, proof) = prove(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs.clone(),
            &mut host,
            self.proof_options.clone(),
        )
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        let (account_delta, output_notes, _tx_progress) = host.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        self.build_proven_transaction(
            input_notes,
            tx_outputs,
            account_delta,
            account,
            ref_block_num,
            ref_block_commitment,
            proof,
        )
    }
}
