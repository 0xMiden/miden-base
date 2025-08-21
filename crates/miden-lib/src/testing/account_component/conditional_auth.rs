use alloc::string::String;

use miden_objects::account::AccountComponent;
use miden_objects::assembly::{Library, default_source_manager_arc_dyn};
use miden_objects::utils::sync::LazyLock;

use crate::transaction::TransactionKernel;

pub const ERR_WRONG_ARGS_MSG: &str = "auth procedure args are incorrect";

static CONDITIONAL_AUTH_CODE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
        use.miden::account

        const.WRONG_ARGS="{ERR_WRONG_ARGS_MSG}"

        export.auth__conditional
            # => [AUTH_ARGS]

            # If [97, 98, 99] is passed as an argument, all good.
            # Otherwise we error out.
            push.97 assert_eq.err=WRONG_ARGS
            push.98 assert_eq.err=WRONG_ARGS
            push.99 assert_eq.err=WRONG_ARGS

            # Last element is the incr_nonce_flag.
            if.true
                exec.account::incr_nonce drop
            end
            dropw dropw dropw dropw
        end
"#
    )
});

static CONDITIONAL_AUTH_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    TransactionKernel::assembler()
        .assemble_library([CONDITIONAL_AUTH_CODE.as_str()])
        .expect("conditional auth code should be valid")
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth__conditional` procedure that conditionally succeeds and
/// conditionally increments the nonce based on the authentication arguments.
///
/// The auth procedure expects the first three arguments as [99, 98, 97] to succeed.
/// In case it succeeds, it conditionally increments the nonce based on the fourth argument.
pub struct ConditionalAuthComponent;

impl From<ConditionalAuthComponent> for AccountComponent {
    fn from(_: ConditionalAuthComponent) -> Self {
        AccountComponent::new(CONDITIONAL_AUTH_LIBRARY.clone(), vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
