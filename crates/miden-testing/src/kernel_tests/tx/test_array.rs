//! Tests for the Array account component's `get` and `set` procedures.

use alloc::sync::Arc;

use miden_lib::account::array::Array;
use miden_lib::utils::CodeBuilder;
use miden_objects::account::{AccountBuilder, AccountComponent, StorageSlotName};
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::{Felt, Word};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::{Auth, TransactionContextBuilder};

/// The slot name used for testing the array component.
const TEST_ARRAY_SLOT: &str = "test::array::data";

/// The component name used for testing the array component.
const TEST_ARRAY_COMPONENT: &str = "test::array::component";

/// Comprehensive test for the Array component that verifies:
/// 1. Initial value can be retrieved via `get`
/// 2. Value can be updated via `set`
/// 3. Updated value can be retrieved via `get`
///
/// Since we cannot use `exec` from a transaction script to invoke account procedures directly,
/// we create a wrapper account component that exposes procedures which internally use `exec`
/// to call the array component's procedures.
#[tokio::test]
async fn test_array_get_and_set() -> anyhow::Result<()> {
    let data_slot = StorageSlotName::new(TEST_ARRAY_SLOT).expect("slot name should be valid");

    // Initialize the array with the first entry (index 0) set to [42, 42, 42, 42]
    let initial_value = Word::from([42u32, 42, 42, 42]);

    // Generate the array library for linking
    let array = Array::with_elements(data_slot.clone(), [(Felt::new(0), initial_value)]);
    let array_library = array.generate_library(TEST_ARRAY_COMPONENT);

    // Create a wrapper account component that uses `exec` to call the array procedures.
    // This wrapper is needed because transaction scripts cannot use `exec` to call
    // account procedures directly - they must use `call`.
    let wrapper_component_code = format!(
        r#"
        use.{component}->test_array

        #! Wrapper for array::get that uses exec internally.
        #! Inputs:  [index, pad(15)]
        #! Outputs: [VALUE, pad(12)]
        export.test_get
            exec.test_array::get
        end

        #! Wrapper for array::set that uses exec internally.
        #! Inputs:  [index, VALUE, pad(11)]
        #! Outputs: [OLD_VALUE, pad(12)]
        export.test_set
            exec.test_array::set
        end
    "#,
        component = TEST_ARRAY_COMPONENT
    );

    // Build the wrapper component by linking against the array library
    let mut assembler: miden_objects::assembly::Assembler =
        CodeBuilder::with_mock_libraries_with_source_manager(Arc::new(
            DefaultSourceManager::default(),
        ))
        .into();
    assembler
        .link_static_library(&array_library)
        .expect("should be able to link array library");

    let wrapper_source = NamedSource::new("wrapper::component", wrapper_component_code);
    let wrapper_library = assembler
        .clone()
        .assemble_library([wrapper_source])
        .expect("wrapper component MASM should be valid");

    // Create the wrapper account component (no storage slots needed for the wrapper itself)
    let wrapper_component =
        AccountComponent::new(wrapper_library.clone(), vec![])?.with_supports_all_types();

    // Build an account with both the Array component and the wrapper component
    let array_for_account =
        Array::with_elements(data_slot.clone(), [(Felt::new(0), initial_value)]);
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(array_for_account)
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
        use.wrapper::component->wrapper

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
