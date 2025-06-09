use alloc::{collections::BTreeMap, sync::Arc};

use miden_objects::{Digest, assembly::mast::MastForest, note::NoteScript};
use vm_processor::MastForestStore;

/// Stores the MAST forests for a set of note scripts.
pub struct NoteMastForestStore {
    mast_forests: BTreeMap<Digest, Arc<MastForest>>,
}

impl NoteMastForestStore {
    pub fn new(notes: impl Iterator<Item = impl AsRef<NoteScript>>) -> Self {
        let mut mast_forests = BTreeMap::new();

        for note in notes {
            let forest = note.as_ref().mast();
            for proc_root in forest.local_procedure_digests() {
                mast_forests.insert(proc_root, forest.clone());
            }
        }

        Self { mast_forests }
    }
}

impl MastForestStore for NoteMastForestStore {
    fn get(&self, procedure_root: &Digest) -> Option<Arc<MastForest>> {
        self.mast_forests.get(procedure_root).cloned()
    }
}
