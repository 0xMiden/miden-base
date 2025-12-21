//! Test an auth component that attempt reentrancy.

use alloc::string::String;

use miden_protocol::Word;
use miden_protocol::account::{
    AccountComponent,
    AccountComponentCode,
    StorageSlot,
    StorageSlotName,
};
use miden_protocol::utils::sync::LazyLock;

use crate::code_builder::CodeBuilder;

/// The storage slot name for the call counter in the self-calling auth component.
pub const CALL_COUNTER_SLOT_NAME: &str = "mock::self_calling_auth::counter";

// SELF-CALLING AUTH COMPONENT
// ================================================================================================

/// An auth component that attempts to call itself recursively.
///
/// This tests whether the kernel properly prevents an auth procedure from calling itself
/// during execution.
// NOTE: MASM does not allow self-recursive procedure calls at the syntax level.
// The assembler will reject `call.auth_self_call` from within `auth_self_call`.
// This is a compile-time protection against direct reentrancy.
//
// However, we can try to bypass this by dynamically obtaining the auth procedure's
// root hash and using `dyncall` to invoke it. This tests whether the kernel's
// runtime checks (was_procedure_called) properly prevent this attack vector.
static SELF_CALLING_AUTH_CODE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
        use miden::protocol::native_account
        use miden::protocol::active_account

        # Storage slot for the call counter
        const CALL_COUNTER_SLOT = word("{CALL_COUNTER_SLOT_NAME}")

        # Auth procedure that attempts to call itself via dynexec
        # by getting its own procedure root and invoking it dynamically.
        #
        # Uses a call counter stored in account storage to avoid infinite recursion:
        # - First call: counter is 0, we increment to 1 and try dynexec
        # - Reentrant call: counter would be 1, so we skip dynexec
        #
        # This allows testing whether the kernel properly prevents the reentrancy from the auth
        # procedure.
        @locals(4)
        pub proc auth_self_call
            # => [AUTH_ARGS]
            dropw

            # Load current call counter from storage (initialized to 0)
            push.CALL_COUNTER_SLOT[0..2] exec.active_account::get_item
            # => [v0, v1, v2, v3] where v0 is at top, counter is stored in v3

            # The counter is the first element
            # => [counter, 0, 0, 0]

            # Increment the counter
            add.1
            # => [counter+1, 0, 0, 0]
            
            # Store the updated counter back to storage
            dup movdn.4
            debug.stack
            push.CALL_COUNTER_SLOT[0..2] exec.native_account::set_item dropw
            # => [counter+1]
            debug.stack

            # Only attempt reentrancy if this is the first call (counter was 0, now is 1)
            push.1 eq
            # => [is_first_call]

            if.true
                # Get our own procedure root (auth procedure is always at index 0)
                push.0
                exec.active_account::get_procedure_root
                # => [AUTH_PROC_ROOT]

                # Store the procedure root to local memory and get pointer for dynexec
                loc_storew_be.0 dropw locaddr.0
                # => [auth_proc_root_ptr]

                # Pad the stack for dynexec (needs 16 elements minimum for inputs)
                padw padw padw push.0.0.0
                # => [pad(15), auth_proc_root_ptr]

                movup.15
                # => [auth_proc_root_ptr, pad(15)]

                # Try to call ourselves via dynexec - this is the reentrancy attempt
                dynexec
                # => [OUTPUT_3, OUTPUT_2, OUTPUT_1, OUTPUT_0]
            else
                # Increment the nonce to make the transaction valid
                # This is done after the reentrancy attempt so that the kernel's auth
                # reentrancy check is hit before the nonce increment check
                exec.native_account::incr_nonce drop
            end

            # Clean up the stack
            dropw dropw dropw dropw
        end
    "#
    )
});

static SELF_CALLING_AUTH_LIBRARY: LazyLock<AccountComponentCode> = LazyLock::new(|| {
    CodeBuilder::default()
        .compile_component_code("mock::self_calling_auth", SELF_CALLING_AUTH_CODE.as_str())
        .expect("self-calling auth code should be valid")
});

/// An auth component that attempts to call itself recursively.
///
/// This is designed to test whether the kernel properly prevents reentrancy
/// when an auth procedure tries to invoke itself during execution.
pub struct SelfCallingAuthComponent;

impl From<SelfCallingAuthComponent> for AccountComponent {
    fn from(_: SelfCallingAuthComponent) -> Self {
        let counter_slot_name = StorageSlotName::new(CALL_COUNTER_SLOT_NAME)
            .expect("counter slot name should be valid");
        let counter_slot = StorageSlot::with_value(counter_slot_name, Word::default());

        AccountComponent::new(SELF_CALLING_AUTH_LIBRARY.clone(), vec![counter_slot])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}
