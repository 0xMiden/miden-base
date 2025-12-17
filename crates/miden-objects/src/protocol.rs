use alloc::sync::Arc;

// RE-EXPORTS
// ================================================================================================
pub use miden_core_lib::CoreLibrary;

use crate::assembly::Library;
use crate::assembly::mast::MastForest;
use crate::utils::serde::Deserializable;
use crate::utils::sync::LazyLock;

// CONSTANTS
// ================================================================================================

const MIDEN_LIB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/assets/miden.masl"));

// MIDEN LIBRARY
// ================================================================================================

#[derive(Clone)]
pub struct MidenLib(Library);

impl MidenLib {
    /// Returns a reference to the [`MastForest`] of the inner [`Library`].
    pub fn mast_forest(&self) -> &Arc<MastForest> {
        self.0.mast_forest()
    }
}

impl AsRef<Library> for MidenLib {
    fn as_ref(&self) -> &Library {
        &self.0
    }
}

impl From<MidenLib> for Library {
    fn from(value: MidenLib) -> Self {
        value.0
    }
}

impl Default for MidenLib {
    fn default() -> Self {
        static MIDEN_LIB: LazyLock<MidenLib> = LazyLock::new(|| {
            let contents =
                Library::read_from_bytes(MIDEN_LIB_BYTES).expect("failed to read miden lib masl!");
            MidenLib(contents)
        });
        MIDEN_LIB.clone()
    }
}

// TESTS
// ================================================================================================

// NOTE: Most protocol-related tests can be found in miden-testing.
#[cfg(all(test, feature = "std"))]
mod tests {
    use super::MidenLib;
    use crate::assembly::Path;

    #[test]
    fn test_compile() {
        let path = Path::new("::miden::active_account::get_id");
        let miden = MidenLib::default();
        let exists = miden.0.module_infos().any(|module| {
            module
                .procedures()
                .any(|(_, proc)| module.path().join(&proc.name).as_path() == path)
        });

        assert!(exists);
    }
}
