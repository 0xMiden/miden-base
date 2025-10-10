use super::auth::AuthScheme;

pub mod auth;
pub mod components;
pub mod faucets;
pub mod interface;
pub mod wallets;

/// Macro to simplify the creation of static procedure digest constants.
///
/// This macro generates a `LazyLock<Word>` static variable that lazily initializes
/// the digest of a procedure from a library.
///
/// # Arguments
/// * `$name` - The name of the static variable to create
/// * `$proc_name` - The string name of the procedure
/// * `$library_fn` - The function that returns the library containing the procedure
///
/// # Example
/// ```ignore
/// procedure_digest!(
///     BASIC_WALLET_RECEIVE_ASSET,
///     BasicWallet::RECEIVE_ASSET_PROC_NAME,
///     basic_wallet_library
/// );
/// ```
#[macro_export]
macro_rules! procedure_digest {
    ($name:ident, $proc_name:expr, $library_fn:expr) => {
        static $name: miden_objects::utils::sync::LazyLock<miden_objects::Word> =
            miden_objects::utils::sync::LazyLock::new(|| {
                let qualified_name = miden_objects::assembly::QualifiedProcedureName::new(
                    ::core::default::Default::default(),
                    miden_objects::assembly::ProcedureName::new($proc_name).unwrap_or_else(|_| {
                        panic!("failed to create name for '{}' procedure", $proc_name)
                    }),
                );
                $library_fn()
                    .get_procedure_root_by_name(qualified_name)
                    .unwrap_or_else(|| {
                        panic!("{} should contain '{}' procedure", stringify!($library_fn), $proc_name)
                    })
            });
    };
}
