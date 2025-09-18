use miden_objects::account::AccountComponent;
use miden_objects::assembly::Library;
use miden_objects::utils::sync::LazyLock;

use crate::transaction::TransactionKernel;

const INCR_NONCE_AUTH_CODE: &str = "
    use.miden::account

    export.auth_incr_nonce
        exec.account::incr_nonce drop
    end
";

static INCR_NONCE_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    TransactionKernel::assembler()
        .assemble_library([INCR_NONCE_AUTH_CODE])
        .expect("incr nonce code should be valid")
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth_incr_nonce` procedure that always increments the nonce by 1.
pub struct IncrNonceAuthComponent;

impl From<IncrNonceAuthComponent> for AccountComponent {
    fn from(_: IncrNonceAuthComponent) -> Self {
        AccountComponent::new(INCR_NONCE_AUTH_LIBRARY.clone(), vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
