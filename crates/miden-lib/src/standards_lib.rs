use alloc::sync::Arc;

use miden_objects::assembly::Library;
use miden_objects::assembly::mast::MastForest;
use miden_objects::utils::serde::Deserializable;
use miden_objects::utils::sync::LazyLock;

// CONSTANTS
// ================================================================================================

const STANDARDS_LIB_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/assets/standards.masl"));

// MIDEN STANDARDS LIBRARY
// ================================================================================================

#[derive(Clone)]
pub struct StandardsLib(Library);

impl StandardsLib {
    /// Returns a reference to the [`MastForest`] of the inner [`Library`].
    pub fn mast_forest(&self) -> &Arc<MastForest> {
        self.0.mast_forest()
    }
}

impl AsRef<Library> for StandardsLib {
    fn as_ref(&self) -> &Library {
        &self.0
    }
}

impl From<StandardsLib> for Library {
    fn from(value: StandardsLib) -> Self {
        value.0
    }
}

impl Default for StandardsLib {
    fn default() -> Self {
        static MIDEN_LIB: LazyLock<StandardsLib> = LazyLock::new(|| {
            let contents = Library::read_from_bytes(STANDARDS_LIB_BYTES)
                .expect("failed to read standards lib masl");
            StandardsLib(contents)
        });
        MIDEN_LIB.clone()
    }
}

// TESTS
// ================================================================================================

// NOTE: Most standards-related tests can be found in miden-testing.
#[cfg(all(test, feature = "std"))]
mod tests {
    use miden_objects::assembly::Path;

    use super::StandardsLib;

    #[test]
    fn test_compile() {
        let path = Path::new("::miden::contracts::faucets::basic_fungible::distribute");
        let miden = StandardsLib::default();
        let exists = miden.0.module_infos().any(|module| {
            module
                .procedures()
                .any(|(_, proc)| module.path().join(&proc.name).as_path() == path)
        });

        assert!(exists);
    }
}
