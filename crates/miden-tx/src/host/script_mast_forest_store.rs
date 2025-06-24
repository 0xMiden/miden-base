use alloc::{collections::BTreeMap, sync::Arc};

use miden_objects::{
    Digest, assembly::mast::MastForest, note::NoteScript, transaction::TransactionScript,
    vm::AdviceMap,
};
use vm_processor::MastForestStore;

/// Stores the MAST forests for a set of scripts (both note scripts and transaction scripts).
///
/// A [ScriptMastForestStore] is meant to exclusively store MAST forests related to both
/// transaction and input note scripts.
pub struct ScriptMastForestStore {
    mast_forests: BTreeMap<Digest, Arc<MastForest>>,
    adv_data: AdviceMap,
}

impl ScriptMastForestStore {
    /// Creates a new [ScriptMastForestStore].
    pub fn new(
        tx_script: Option<&TransactionScript>,
        note_scripts: impl Iterator<Item = impl AsRef<NoteScript>>,
    ) -> Self {
        let mut mast_store = ScriptMastForestStore {
            mast_forests: BTreeMap::new(),
            adv_data: AdviceMap::new(),
        };

        for note_script in note_scripts {
            mast_store.insert(note_script.as_ref().mast());
        }

        if let Some(tx_script) = tx_script {
            mast_store.insert(tx_script.mast());
        }
        mast_store
    }

    /// Registers all procedures of the provided [MastForest] with this store.
    fn insert(&mut self, mast_forest: Arc<MastForest>) {
        // only register procedures that are local to this forest
        for proc_digest in mast_forest.local_procedure_digests() {
            self.mast_forests.insert(proc_digest, mast_forest.clone());
        }

        // collect advice data from the forest
        for (key, values) in mast_forest.advice_map().clone() {
            self.adv_data.insert((*key).into(), values);
        }
    }

    /// Returns a reference to the advice data collected from all forests.
    pub fn advice_data(&self) -> &AdviceMap {
        &self.adv_data
    }
}

// MAST FOREST STORE IMPLEMENTATION
// ================================================================================================

impl MastForestStore for ScriptMastForestStore {
    fn get(&self, procedure_root: &Digest) -> Option<Arc<MastForest>> {
        self.mast_forests.get(procedure_root).cloned()
    }
}
