// ACCOUNT CODE
// ================================================================================================

use assembly::Assembler;

use crate::account::{AccountCode, AccountComponent, AccountType};
use crate::testing::noop_auth_component::NoopAuthComponent;

pub const CODE: &str = "
    export.foo
        push.1 push.2 mul
    end

    export.bar
        push.1 push.2 add
    end
";

impl AccountCode {
    /// Creates a mock [AccountCode] with default assembler and mock code
    pub fn mock() -> AccountCode {
        let component = AccountComponent::compile(CODE, Assembler::default(), vec![])
            .unwrap()
            .with_supports_all_types();

        Self::from_components(
            &[NoopAuthComponent.into(), component],
            AccountType::RegularAccountUpdatableCode,
        )
        .unwrap()
    }
}
