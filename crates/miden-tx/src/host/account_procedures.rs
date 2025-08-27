use alloc::string::ToString;

use miden_lib::transaction::memory::{ACCOUNT_STACK_TOP_PTR, ACCT_CODE_COMMITMENT_OFFSET};
use miden_lib::transaction::{TransactionAdviceInputs, TransactionKernelError};
use miden_objects::account::{AccountCode, AccountProcedureInfo};
use miden_objects::transaction::{TransactionArgs, TransactionInputs};
use miden_processor::AdviceInputs;

use super::{BTreeMap, Felt, ProcessState, Word};
use crate::errors::TransactionHostError;

// ACCOUNT PROCEDURE INDEX MAP
// ================================================================================================

/// A map of maps { acct_code_commitment |-> { proc_root |-> proc_index } } for all known
/// procedures of account interfaces for all accounts expected to be invoked during transaction
/// execution.
pub struct AccountProcedureIndexMap(BTreeMap<Word, BTreeMap<Word, u8>>);

impl AccountProcedureIndexMap {
    /// Returns a new [AccountProcedureIndexMap] instantiated with account procedures present in
    /// the provided advice provider.
    ///
    /// Note: `account_code_commitments` iterator should include both native account code and
    /// foreign account codes commitments
    pub fn new(
        account_code_commitments: impl IntoIterator<Item = Word>,
        adv_provider: &AdviceInputs,
    ) -> Result<Self, TransactionHostError> {
        let mut result = BTreeMap::new();

        for code_commitment in account_code_commitments {
            let account_procs_map = build_account_procedure_map(code_commitment, adv_provider)?;
            result.insert(code_commitment, account_procs_map);
        }

        Ok(Self(result))
    }

    /// Builds an [`AccountProcedureIndexMap`] for the specified transaction inputs and arguments.
    ///
    /// The resulting instance will map all account code commmitments to a mapping of
    /// `proc_root |-> proc_index` for any account that is expected to be involved in the
    /// transaction, enabling easy procedure index lookups on runtime.
    pub fn from_transaction_params(
        tx_inputs: &TransactionInputs,
        tx_args: &TransactionArgs,
        tx_advice_inputs: &TransactionAdviceInputs,
    ) -> Result<Self, TransactionHostError> {
        let mut account_code_commitments = tx_args.to_foreign_account_code_commitments();
        account_code_commitments.insert(tx_inputs.account().code().commitment());

        Self::new(account_code_commitments, tx_advice_inputs.as_advice_inputs())
    }

    /// Returns index of the procedure whose root is currently at the top of the operand stack in
    /// the provided process.
    ///
    /// # Errors
    /// Returns an error if the procedure at the top of the operand stack is not present in this
    /// map.
    pub fn get_proc_index(&self, process: &ProcessState) -> Result<u8, TransactionKernelError> {
        // get current account code commitment
        let code_commitment = {
            let account_stack_top_ptr = process
                .get_mem_value(process.ctx(), ACCOUNT_STACK_TOP_PTR)
                .expect("Account stack top pointer was not initialized")
                .as_int();
            let curr_data_ptr = process
                .get_mem_value(
                    process.ctx(),
                    account_stack_top_ptr
                        .try_into()
                        .expect("account stack top pointer should be less than u32::MAX"),
                )
                .expect("Current account pointer was not initialized")
                .as_int();
            process
                .get_mem_word(process.ctx(), curr_data_ptr as u32 + ACCT_CODE_COMMITMENT_OFFSET)
                .expect("failed to read a word from memory")
                .expect("current account code commitment was not initialized")
        };

        let proc_root = process.get_stack_word(0);

        self.0
            .get(&code_commitment)
            .ok_or(TransactionKernelError::UnknownCodeCommitment(code_commitment))?
            .get(&proc_root)
            .cloned()
            .ok_or(TransactionKernelError::UnknownAccountProcedure(proc_root))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

fn build_account_procedure_map(
    code_commitment: Word,
    advice_inputs: &AdviceInputs,
) -> Result<BTreeMap<Word, u8>, TransactionHostError> {
    // get the account procedures from the advice_map
    let proc_data = advice_inputs.map.get(&code_commitment).ok_or_else(|| {
        TransactionHostError::AccountProcedureIndexMapError(
            "failed to read account procedure data from the advice provider".to_string(),
        )
    })?;

    let mut account_procs_map = BTreeMap::new();

    // sanity checks

    // check that there are procedures in the account code
    if proc_data.is_empty() {
        return Err(TransactionHostError::AccountProcedureIndexMapError(
            "account code does not contain any procedures.".to_string(),
        ));
    }

    // check that procedure data have a correct length
    if proc_data.len() % AccountProcedureInfo::NUM_ELEMENTS_PER_PROC != 0 {
        return Err(TransactionHostError::AccountProcedureIndexMapError(
            "account procedure data has invalid length.".to_string(),
        ));
    }

    // One procedure requires 8 values to represent
    let num_procs = proc_data.len() / AccountProcedureInfo::NUM_ELEMENTS_PER_PROC;

    // check that the account code does not contain too many procedures
    if num_procs > AccountCode::MAX_NUM_PROCEDURES {
        return Err(TransactionHostError::AccountProcedureIndexMapError(
            "account code contains too many procedures.".to_string(),
        ));
    }

    for (proc_idx, proc_info) in
        proc_data.chunks_exact(AccountProcedureInfo::NUM_ELEMENTS_PER_PROC).enumerate()
    {
        let proc_info_array: [Felt; AccountProcedureInfo::NUM_ELEMENTS_PER_PROC] =
            proc_info.try_into().expect("Failed conversion into procedure info array.");

        let procedure = AccountProcedureInfo::try_from(proc_info_array)
            .map_err(TransactionHostError::AccountProcedureInfoCreationFailed)?;

        let proc_idx = u8::try_from(proc_idx).expect("Invalid procedure index.");

        account_procs_map.insert(*procedure.mast_root(), proc_idx);
    }

    Ok(account_procs_map)
}
