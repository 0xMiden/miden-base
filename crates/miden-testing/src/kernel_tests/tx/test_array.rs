//! Tests for the Array utility `get` and `set` procedures.

use miden_protocol::account::{
    AccountBuilder,
    AccountComponent,
    StorageMap,
    StorageSlot,
    StorageSlotName,
};
use miden_protocol::{Felt, FieldElement, Word};
use miden_standards::code_builder::CodeBuilder;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::{Auth, TransactionContextBuilder};

/// The slot name used for testing the array component.
const TEST_ARRAY_SLOT: &str = "test::array::data";

/// Comprehensive test for the Array utility that verifies:
/// 1. Initial value can be retrieved via `get`
/// 2. Value can be updated via `set`
/// 3. Updated value can be retrieved via `get`
///
/// Since we cannot use `exec` from a transaction script to invoke account procedures directly,
/// we create a wrapper account component that exposes procedures which internally use `exec`
/// to call the array procedures.
#[tokio::test]
async fn test_array_get_and_set() -> anyhow::Result<()> {
    let data_slot = StorageSlotName::new(TEST_ARRAY_SLOT).expect("slot name should be valid");

    // Initialize the array with the first entry (index 0) set to [42, 42, 42, 42]
    let initial_value = Word::from([42u32, 42, 42, 42]);

    // Create a wrapper account component that uses `exec` to call the array procedures.
    // This wrapper is needed because transaction scripts cannot use `exec` to call
    // account procedures directly - they must use `call`.
    let wrapper_component_code = format!(
        r#"
        use miden::core::word
        use miden::standards::data_structures::array

        const ARRAY_SLOT = word("{slot}")

        #! Wrapper for array::get that uses exec internally.
        #! Inputs:  [index, pad(15)]
        #! Outputs: [VALUE, pad(12)]
        pub proc test_get
            push.ARRAY_SLOT[0..2]
            # Drop two padding elements to keep stack depth at 16.
            movup.4 drop
            movup.3 drop
            exec.array::get
        end

        #! Wrapper for array::set that uses exec internally.
        #! Inputs:  [index, VALUE, pad(11)]
        #! Outputs: [OLD_VALUE, pad(12)]
        pub proc test_set
            push.ARRAY_SLOT[0..2]
            exec.array::set
        end
    "#,
        slot = TEST_ARRAY_SLOT
    );

    // Build the wrapper component by linking against the array library
    let wrapper_library =
        CodeBuilder::default().compile_component_code("wrapper::component", wrapper_component_code)?;

    // Create the wrapper account component (no storage slots needed for the wrapper itself)
    let wrapper_component = AccountComponent::new(
        wrapper_library.clone(),
        vec![StorageSlot::with_map(
            data_slot.clone(),
            StorageMap::with_entries([(
                Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::ZERO]),
                initial_value,
            )])?,
        )],
    )?
    .with_supports_all_types();

    // Build an account with both the Array component and the wrapper component
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(wrapper_component)
        .build_existing()?;

    // Verify the storage slot exists
    assert!(
        account.storage().get(&data_slot).is_some(),
        "Array data slot should exist in account storage"
    );

    // Transaction script that:
    // 1. Gets the initial value at index 0 (should be [42, 42, 42, 42])
    // 2. Sets index 0 to [43, 43, 43, 43]
    // 3. Gets the updated value at index 0 (should be [43, 43, 43, 43])
    let tx_script_code = r#"
        use wrapper::component->wrapper

        begin
            # Step 1: Get value at index 0 (should return [42, 42, 42, 42])
            push.0
            # => [index, pad(16)]
            call.wrapper::test_get
            # => [VALUE, pad(13)]

            # Verify value is [42, 42, 42, 42]
            push.42.42.42.42
            assert_eqw.err="get(0) should return [42, 42, 42, 42] initially"
            # => [pad(16)] (auto-padding)

            # Step 2: Set value at index 0 to [43, 43, 43, 43]
            push.43.43.43.43
            push.0
            # => [index, VALUE, pad(16)]
            call.wrapper::test_set
            # => [OLD_VALUE, pad(17)]
            dropw
            
            # Step 3: Get value at index 0 (should return [43, 43, 43, 43])
            push.0
            # => [index, pad(17)]
            call.wrapper::test_get
            # => [VALUE, pad(14)]
            
            # Verify value is [43, 43, 43, 43]
            push.43.43.43.43
            assert_eqw.err="get(0) should return [43, 43, 43, 43] after set"
            # => [pad(16)] (auto-padding)
        end
        "#;

    // Compile the transaction script with the wrapper library linked
    let tx_script = CodeBuilder::default()
        .with_dynamically_linked_library(&wrapper_library)?
        .compile_tx_script(tx_script_code)?;

    // Create transaction context and execute
    let tx_context = TransactionContextBuilder::new(account).tx_script(tx_script).build()?;

    tx_context.execute().await?;

    Ok(())
}
