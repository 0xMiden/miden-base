#[cfg(feature = "async")]
use alloc::boxed::Box;
use alloc::{collections::BTreeSet, sync::Arc, vec::Vec};

use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::delta::AccountUpdateDetails,
    assembly::DefaultSourceManager,
    transaction::{OutputNote, ProvenTransaction, ProvenTransactionBuilder, TransactionWitness},
};
pub use miden_prover::ProvingOptions;
use miden_prover::prove;
use vm_processor::{Digest, MemAdviceProvider};
use winter_maybe_async::*;

use super::{TransactionHost, TransactionProverError};

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
                .map_err(TransactionProverError::InvalidTransactionInputs)?;
        let advice_provider: MemAdviceProvider = advice_inputs.into_inner().into();

        // load the store with account/note/tx_script MASTs
        self.mast_store.load_transaction_code(account.code(), input_notes, &tx_args);

        let account_code_commitments: BTreeSet<Digest> = tx_args
            .foreign_account_inputs()
            .iter()
            .map(|acc| acc.code().commitment())
            .collect();

        let mut host: TransactionHost<_> = TransactionHost::new(
            account.into(),
            advice_provider,
            self.mast_store.clone(),
            None,
            account_code_commitments,
        )
        .map_err(TransactionProverError::TransactionHostCreationFailed)?;

        // For the prover, we assume that the transaction witness was successfully executed and so
        // there is no need to provide the actual source manager, as it is only used to improve
        // error quality. So we simply pass an empty one.
        let source_manager = Arc::new(DefaultSourceManager::default());
        let (stack_outputs, proof) = maybe_await!(prove(
            &TransactionKernel::main(),
            stack_inputs,
            &mut host,
            self.proof_options.clone(),
            source_manager
        ))
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        // extract transaction outputs and process transaction data
        let (advice_provider, account_delta, output_notes, _signatures, _tx_progress) =
            host.into_parts();
        let (_, map, _) = advice_provider.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &map.into(), output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        // erase private note information (convert private full notes to just headers)
        let output_notes: Vec<_> = tx_outputs.output_notes.iter().map(OutputNote::shrink).collect();

        let builder = ProvenTransactionBuilder::new(
            account.id(),
            account.init_commitment(),
            tx_outputs.account.commitment(),
            ref_block_num,
            ref_block_commitment,
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
