use crate::account::AccountComponent;
use crate::assembly::{Assembler, Library};
use crate::utils::sync::LazyLock;

// ADD COMPONENT
// ================================================================================================

const ADD_CODE: &str = "
    export.add5
        add.5
    end
";

static ADD_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    Assembler::default()
        .assemble_library([ADD_CODE])
        .expect("add code should be valid")
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `add5` procedure that adds 5 to its input.
pub struct AddComponent;

impl From<AddComponent> for AccountComponent {
    fn from(_: AddComponent) -> Self {
        AccountComponent::new(ADD_LIBRARY.clone(), vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
