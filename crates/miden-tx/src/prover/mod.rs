#[cfg(feature = "async")]
use alloc::boxed::Box;
use alloc::{collections::BTreeSet, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::{Account, AccountDelta, delta::AccountUpdateDetails},
    assembly::DefaultSourceManager,
    block::BlockNumber,
    transaction::{
        InputNote, InputNotes, OutputNote, ProvenTransaction, ProvenTransactionBuilder,
        TransactionOutputs, TransactionWitness,
    },
};
pub use miden_prover::ProvingOptions;
use miden_prover::{ExecutionProof, prove};
use vm_processor::Word;
use winter_maybe_async::*;

use super::TransactionProverError;
use crate::host::ScriptMastForestStore;

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

    #[maybe_async]
    #[cfg(any(feature = "testing", test))]
    fn prove_dummy(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        use miden_prover::Proof;

        let TransactionWitness { tx_inputs, account_delta, tx_outputs, .. } = tx_witness;

        self.build_proven_transaction(
            &tx_inputs.input_notes(),
            tx_outputs,
            account_delta,
            tx_inputs.account(),
            tx_inputs.block_header().block_num(),
            tx_inputs.block_header().commitment(),
            ExecutionProof::new(Proof::new_dummy(), Default::default()),
        )
    }

    #[maybe_async]
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

#[maybe_async_trait]
impl TransactionProver for LocalTransactionProver {
    #[maybe_async]
    fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let TransactionWitness { tx_inputs, tx_args, advice_witness, .. } = tx_witness;

        let account = tx_inputs.account();
        let input_notes = tx_inputs.input_notes();
        let ref_block_num = tx_inputs.block_header().block_num();
        let ref_block_commitment = tx_inputs.block_header().commitment();

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_witness));
        let mut advice_inputs = advice_inputs.into_advice_inputs();

        self.mast_store.load_account_code(account.code());

        let account_code_commitments: BTreeSet<Word> = tx_args.foreign_account_code_commitments();
        let script_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            input_notes.iter().map(|n| n.note().script()),
        );

        let mut host = TransactionProverHost::new(
            &account.into(),
            input_notes.clone(),
            &mut advice_inputs,
            self.mast_store.as_ref(),
            script_mast_store,
            account_code_commitments,
        )
        .map_err(TransactionProverError::TransactionHostCreationFailed)?;

        let source_manager = Arc::new(DefaultSourceManager::default());
        let (stack_outputs, proof) = maybe_await!(prove(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs.clone(),
            &mut host,
            self.proof_options.clone(),
            source_manager,
        ))
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        let (account_delta, output_notes, _tx_progress) = host.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        self.build_proven_transaction(
            &input_notes,
            tx_outputs,
            account_delta,
            account,
            ref_block_num,
            ref_block_commitment,
            proof,
        )
    }
}
