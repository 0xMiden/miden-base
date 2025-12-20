use crate::account::AccountComponent;
use crate::assembly::{Assembler, Library};
use crate::utils::sync::LazyLock;

// NOOP AUTH COMPONENT
// ================================================================================================

const NOOP_AUTH_CODE: &str = "
    pub proc auth_noop
        push.0 drop
    end
";

static NOOP_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    Assembler::default()
        .assemble_library([NOOP_AUTH_CODE])
        .expect("noop auth code should be valid")
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth_noop` procedure that does nothing (always succeeds).
pub struct NoopAuthComponent;

impl From<NoopAuthComponent> for AccountComponent {
    fn from(_: NoopAuthComponent) -> Self {
        AccountComponent::new(NOOP_AUTH_LIBRARY.clone(), vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
