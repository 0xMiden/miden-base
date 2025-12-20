use miden_objects::account::AccountComponent;
use miden_objects::assembly::Library;
use miden_objects::utils::sync::LazyLock;

use crate::utils::CodeBuilder;

const INCR_NONCE_AUTH_CODE: &str = "
    use miden::native_account

    pub proc auth_incr_nonce
        exec.native_account::incr_nonce drop
    end
";

static INCR_NONCE_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    CodeBuilder::default()
        .compile_component_code("incr_nonce", INCR_NONCE_AUTH_CODE)
        .expect("incr nonce code should be valid")
        .into_library()
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes under the "incr_nonce"
/// namespace.
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
