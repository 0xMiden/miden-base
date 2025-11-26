#![no_std]

use alloc::sync::Arc;

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use miden_objects::assembly::Library;
use miden_objects::assembly::mast::MastForest;
use miden_objects::utils::serde::Deserializable;
use miden_objects::utils::sync::LazyLock;

mod auth;
pub use auth::AuthScheme;

pub mod account;
pub mod agglayer;
pub mod block;
pub mod errors;
pub mod note;
pub mod transaction;
pub mod utils;

#[cfg(any(feature = "testing", test))]
pub mod testing;

// RE-EXPORTS
// ================================================================================================
pub use miden_stdlib::StdLibrary;

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

// NOTE: Most kernel-related tests can be found under /miden-tx/kernel_tests
#[cfg(all(test, feature = "std"))]
mod tests {
    use miden_objects::assembly::LibraryPath;

    use super::MidenLib;

    #[test]
    fn test_compile() {
        let path = "miden::active_account::get_id".parse::<LibraryPath>().unwrap();
        let miden = MidenLib::default();
        let exists = miden.0.module_infos().any(|module| {
            module
                .procedures()
                .any(|(_, proc)| module.path().clone().append(&proc.name).unwrap() == path)
        });

        assert!(exists);
    }
}
