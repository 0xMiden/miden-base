//! Tests for the Array account component's `get` and `set` procedures.

use miden_lib::account::array::Array;
use miden_lib::utils::CodeBuilder;
use miden_objects::account::{AccountBuilder, StorageSlotName};
use miden_objects::{Felt, Word};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::{Auth, TransactionContextBuilder};

/// The slot name used for testing the array component.
const TEST_ARRAY_SLOT: &str = "test::array::data";

/// Comprehensive test for the Array component that verifies:
/// 1. Initial value can be retrieved via `get`
/// 2. Value can be updated via `set`
/// 3. Updated value can be retrieved via `get`
#[tokio::test]
async fn test_array_get_and_set() -> anyhow::Result<()> {
    let data_slot = StorageSlotName::new(TEST_ARRAY_SLOT).expect("slot name should be valid");

    // Initialize the array with the first entry (index 0) set to [42, 42, 42, 42]
    let initial_value = Word::from([42u32, 42, 42, 42]);
    let array = Array::with_elements(data_slot.clone(), [(Felt::new(0), initial_value)]);
    let array_library = array.generate_library();

    // Build an account with the Array component (need a new instance for the account)
    let array_for_account =
        Array::with_elements(data_slot.clone(), [(Felt::new(0), initial_value)]);
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(array_for_account)
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
        use.array::component->test_array

        begin
            # Step 1: Get value at index 0 (should return [42, 42, 42, 42])
            padw padw padw push.0.0.0
            push.0
            call.test_array::get

            # Verify value is [42, 42, 42, 42]
            push.42.42.42.42
            assert_eqw.err="get(0) should return [42, 42, 42, 42] initially"
            dropw dropw dropw

            # Step 2: Set value at index 0 to [43, 43, 43, 43]
            padw padw push.0.0.0
            push.43.43.43.43
            push.0
            call.test_array::set
            dropw dropw dropw dropw  # drop OLD_MAP_ROOT, OLD_MAP_VALUE, pad

            # Step 3: Get value at index 0 (should return [43, 43, 43, 43])
            padw padw padw push.0.0.0
            push.0
            call.test_array::get

            # Verify value is [43, 43, 43, 43]
            push.43.43.43.43
            assert_eqw.err="get(0) should return [43, 43, 43, 43] after set"
            dropw dropw dropw
        end
        "#;

    // Compile the transaction script with the array library linked
    let tx_script = CodeBuilder::default()
        .with_statically_linked_library(&array_library)?
        .compile_tx_script(tx_script_code)?;

    // Create transaction context and execute
    let tx_context = TransactionContextBuilder::new(account).tx_script(tx_script).build()?;

    tx_context.execute().await?;

    Ok(())
}
