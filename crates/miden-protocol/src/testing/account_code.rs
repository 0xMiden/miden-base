// ACCOUNT CODE
// ================================================================================================

use miden_assembly::Assembler;

use crate::account::{AccountCode, AccountComponent, AccountType};
use crate::testing::noop_auth_component::NoopAuthComponent;

pub const CODE: &str = "
    pub proc foo
        push.1.2 mul
    end

    pub proc bar
        push.1.2 add
    end
";

impl AccountCode {
    /// Creates a mock [AccountCode] with default assembler and mock code
    pub fn mock() -> AccountCode {
        let library = Assembler::default()
            .assemble_library([CODE])
            .expect("mock account component should assemble");
        let component = AccountComponent::new(library, vec![]).unwrap().with_supports_all_types();

        Self::from_components(
            &[NoopAuthComponent.into(), component],
            AccountType::RegularAccountUpdatableCode,
        )
        .unwrap()
    }
}
