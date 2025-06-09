use alloc::{collections::BTreeMap, sync::Arc};

use miden_lib::{MidenLib, StdLibrary, transaction::TransactionKernel, utils::sync::RwLock};
use miden_objects::{
    Digest, account::AccountCode, assembly::mast::MastForest, transaction::TransactionArgs,
};
use vm_processor::MastForestStore;

// TRANSACTION MAST STORE
// ================================================================================================

/// A store for the code available during transaction execution.
///
/// Transaction MAST store contains a map between procedure MAST roots and [MastForest]s containing
/// MASTs for these procedures. The VM will request [MastForest]s from the store when it encounters
/// a procedure which it doesn't have the code for. Thus, to execute a program which makes
/// references to external procedures, the store must be loaded with [MastForest]s containing these
/// procedures.
pub struct TransactionMastStore {
    mast_forests: RwLock<BTreeMap<Digest, Arc<MastForest>>>,
}

#[allow(clippy::new_without_default)]
impl TransactionMastStore {
    /// Returns a new [TransactionMastStore] instantiated with the default libraries.
    ///
    /// The default libraries include:
    /// - Miden standard library (miden-stdlib).
    /// - Miden protocol library (miden-lib).
    /// - Transaction kernel.
    pub fn new() -> Self {
        let mast_forests = RwLock::new(BTreeMap::new());
        let store = Self { mast_forests };

        // load transaction kernel MAST forest
        let kernels_forest = TransactionKernel::kernel().mast_forest().clone();
        store.insert(kernels_forest);

        // load miden-stdlib MAST forest
        let miden_stdlib_forest = StdLibrary::default().mast_forest().clone();
        store.insert(miden_stdlib_forest);

        // load miden lib MAST forest
        let miden_lib_forest = MidenLib::default().mast_forest().clone();
        store.insert(miden_lib_forest);

        store
    }

    /// Loads code required for executing a transaction with the specified inputs and args into
    /// this store.
    ///
    /// The loaded code includes:
    /// - Account code for the account specified from the provided [AccountCode].
    /// - Transaction script (if any) from the specified [TransactionArgs].
    pub fn load_transaction_code(&self, account_code: &AccountCode, tx_args: &TransactionArgs) {
        // load account code
        self.load_account_code(account_code);

        // add extra account codes
        for foreign_account in tx_args.foreign_account_inputs() {
            self.load_account_code(foreign_account.code());
        }

        // load tx script MAST into the MAST store
        if let Some(tx_script) = tx_args.tx_script() {
            self.insert(tx_script.mast().clone());
        }
    }

    /// Registers all procedures of the provided [MastForest] with this store.
    pub fn insert(&self, mast_forest: Arc<MastForest>) {
        let mut mast_forests = self.mast_forests.write();

        // only register procedures that are local to this forest
        for proc_digest in mast_forest.local_procedure_digests() {
            mast_forests.insert(proc_digest, mast_forest.clone());
        }
    }

    /// Loads the provided account code into this store.
    fn load_account_code(&self, code: &AccountCode) {
        self.insert(code.mast().clone());
    }
}

// MAST FOREST STORE IMPLEMENTATION
// ================================================================================================

impl MastForestStore for TransactionMastStore {
    fn get(&self, procedure_root: &Digest) -> Option<Arc<MastForest>> {
        self.mast_forests.read().get(procedure_root).cloned()
    }
}
