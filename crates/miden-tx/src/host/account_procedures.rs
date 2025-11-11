use miden_lib::transaction::memory::{ACCOUNT_STACK_TOP_PTR, ACCT_CODE_COMMITMENT_OFFSET};
use miden_objects::account::AccountCode;

use super::{BTreeMap, ProcessState, Word};
use crate::errors::{TransactionHostError, TransactionKernelError};

// ACCOUNT PROCEDURE INDEX MAP
// ================================================================================================

/// A map of maps { acct_code_commitment |-> { proc_root |-> proc_index } } for all known
/// procedures of account interfaces for all accounts expected to be invoked during transaction
/// execution.
#[derive(Debug, Clone, Default)]
pub struct AccountProcedureIndexMap(BTreeMap<Word, BTreeMap<Word, u8>>);

impl AccountProcedureIndexMap {
    /// Returns a new [`AccountProcedureIndexMap`] instantiated with account procedures from the
    /// provided iterator of [`AccountCode`].
    pub fn new<'code>(
        account_codes: impl IntoIterator<Item = &'code AccountCode>,
    ) -> Result<Self, TransactionHostError> {
        let mut index_map = Self::default();

        for account_code in account_codes {
            // Insert each account procedures only once.
            if !index_map.0.contains_key(&account_code.commitment()) {
                index_map.insert_code(account_code)?;
            }
        }

        Ok(index_map)
    }

    /// Inserts the procedures from the provided [`AccountCode`] into the advice inputs, using
    /// [`AccountCode::commitment`] as the key.
    ///
    /// The resulting instance will map the account code commitment to a mapping of
    /// `proc_root |-> proc_index` for any account that is expected to be involved in the
    /// transaction, enabling fast procedure index lookups at runtime.
    pub fn insert_code(&mut self, code: &AccountCode) -> Result<(), TransactionHostError> {
        let mut procedure_map = BTreeMap::new();
        for (proc_idx, proc_info) in code.procedures().iter().enumerate() {
            let proc_idx = u8::try_from(proc_idx).map_err(|_| {
                TransactionHostError::AccountProcedureIndexMapError(
                    "procedure index out of bounds".into(),
                )
            })?;

            procedure_map.insert(*proc_info.mast_root(), proc_idx);
        }

        self.0.insert(code.commitment(), procedure_map);

        Ok(())
    }

    /// Returns index of the procedure whose root is currently at the top of the operand stack in
    /// the provided process.
    ///
    /// # Errors
    /// Returns an error if the procedure at the top of the operand stack is not present in this
    /// map.
    pub fn get_proc_index(&self, process: &ProcessState) -> Result<u8, TransactionKernelError> {
        // get active account code commitment
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
                .expect("active account pointer was not initialized")
                .as_int();
            process
                .get_mem_word(process.ctx(), curr_data_ptr as u32 + ACCT_CODE_COMMITMENT_OFFSET)
                .expect("failed to read a word from memory")
                .expect("active account code commitment was not initialized")
        };

        let proc_root = process.get_stack_word_be(1);

        self.0
            .get(&code_commitment)
            .ok_or(TransactionKernelError::UnknownCodeCommitment(code_commitment))?
            .get(&proc_root)
            .cloned()
            .ok_or(TransactionKernelError::UnknownAccountProcedure(proc_root))
    }
}
