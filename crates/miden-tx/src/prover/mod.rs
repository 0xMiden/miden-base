use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::TransactionKernel;
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::account::{
    Account,
    AccountCode,
    AccountDelta,
    AccountStorage,
    PartialAccount,
    PartialStorage,
    PartialStorageMap,
    StorageMap,
    StorageSlot,
    StorageSlotType,
};
use miden_objects::asset::{Asset, AssetVault};
use miden_objects::block::BlockNumber;
use miden_objects::crypto::merkle::SmtLeaf;
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

    fn build_proven_transaction(
        &self,
        input_notes: &InputNotes<InputNote>,
        tx_outputs: TransactionOutputs,
        pre_fee_account_delta: AccountDelta,
        account: PartialAccount,
        ref_block_num: BlockNumber,
        ref_block_commitment: Word,
        proof: ExecutionProof,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        // erase private note information (convert private full notes to just headers)
        let output_notes: Vec<_> = tx_outputs.output_notes.iter().map(OutputNote::shrink).collect();

        // Compute the commitment of the pre-fee delta, which goes into the proven transaction,
        // since it is the output of the transaction and so is needed for proof verification.
        let pre_fee_delta_commitment: Word = pre_fee_account_delta.to_commitment();

        let builder = ProvenTransactionBuilder::new(
            account.id(),
            account.initial_commitment(),
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

        let builder = match account.has_public_state() {
            true => {
                let account_update_details = if account.is_new() {
                    let mut account = partial_account_to_full(account);
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

    pub fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let TransactionWitness {
            tx_inputs,
            tx_args,
            foreign_account_code,
            advice_witness,
        } = tx_witness;

        let (stack_inputs, advice_inputs) =
            TransactionKernel::prepare_inputs(&tx_inputs, &tx_args, Some(advice_witness))
                .map_err(TransactionProverError::ConflictingAdviceMapEntry)?;

        self.mast_store.load_account_code(tx_inputs.account().code());
        for account_code in &foreign_account_code {
            self.mast_store.load_account_code(account_code);
        }

        let script_mast_store = ScriptMastForestStore::new(
            tx_args.tx_script(),
            tx_inputs.input_notes().iter().map(|n| n.note().script()),
        );

        let account_procedure_index_map = AccountProcedureIndexMap::from_transaction_params(
            &tx_inputs,
            foreign_account_code.iter().map(AccountCode::commitment).collect(),
            &advice_inputs,
        )
        .map_err(TransactionProverError::TransactionHostCreationFailed)?;

        let (partial_account, _, ref_block, _, input_notes) = tx_inputs.into_parts();
        let mut host = TransactionProverHost::new(
            &partial_account,
            input_notes,
            self.mast_store.as_ref(),
            script_mast_store,
            account_procedure_index_map,
        );

        let advice_inputs = advice_inputs.into_advice_inputs();

        let (stack_outputs, proof) = prove(
            &TransactionKernel::main(),
            stack_inputs,
            advice_inputs.clone(),
            &mut host,
            self.proof_options.clone(),
        )
        .map_err(TransactionProverError::TransactionProgramExecutionFailed)?;

        // Extract transaction outputs and process transaction data.
        // Note that the account delta does not contain the removed transaction fee, so it is the
        // "pre-fee" delta of the transaction.
        let (pre_fee_account_delta, input_notes, output_notes, _tx_progress) = host.into_parts();
        let tx_outputs =
            TransactionKernel::from_transaction_parts(&stack_outputs, &advice_inputs, output_notes)
                .map_err(TransactionProverError::TransactionOutputConstructionFailed)?;

        self.build_proven_transaction(
            &input_notes,
            tx_outputs,
            pre_fee_account_delta,
            partial_account,
            ref_block.block_num(),
            ref_block.commitment(),
            proof,
        )
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

fn partial_account_to_full(partial_account: PartialAccount) -> Account {
    let (id, nonce, code, partial_storage, partial_vault) = partial_account.into_parts();

    // For new accounts, the partial storage must represent the full initial account
    // storage.
    let storage = partial_storage_to_full(partial_storage);

    // The vault of a new account should be empty.
    debug_assert_eq!(partial_vault.leaves().count(), 0);
    let vault = AssetVault::default();

    Account::from_parts(id, vault, storage, code, nonce)
}

fn partial_storage_to_full(partial_storage: PartialStorage) -> AccountStorage {
    let (_, header, mut maps) = partial_storage.into_parts();
    let mut storage_slots = Vec::new();
    for (slot_type, slot_value) in header.slots() {
        match slot_type {
            StorageSlotType::Value => {
                storage_slots.push(StorageSlot::Value(*slot_value));
            },
            StorageSlotType::Map => {
                let storage_map = maps
                    .remove(slot_value)
                    .map(partial_storage_map_to_storage_map)
                    .expect("partial storage map should be present in partial storage");
                storage_slots.push(StorageSlot::Map(storage_map));
            },
        }
    }

    AccountStorage::new(storage_slots)
        .expect("partial storage should not contain more than max allowed storage slots")
}

fn partial_storage_map_to_storage_map(partial_storage_map: PartialStorageMap) -> StorageMap {
    let mut storage_map = StorageMap::new();
    for (key, value) in
        partial_storage_map.leaves().map(|(_, leaf)| leaf).flat_map(SmtLeaf::entries)
    {
        storage_map.insert(*key, *value);
    }
    storage_map
}

#[cfg(any(feature = "testing", test))]
impl LocalTransactionProver {
    pub fn prove_dummy(
        &self,
        executed_tx: ExecutedTransaction,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        let (account_delta, tx_outputs, tx_witness, _) = executed_tx.into_parts();

        let (partial_account, _, ref_block, _, input_notes) = tx_witness.tx_inputs.into_parts();

        self.build_proven_transaction(
            &input_notes,
            tx_outputs,
            account_delta,
            partial_account,
            ref_block.block_num(),
            ref_block.commitment(),
            ExecutionProof::new_dummy(),
        )
    }
}
