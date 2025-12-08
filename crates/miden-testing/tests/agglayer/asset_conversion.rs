extern crate alloc;

use alloc::sync::Arc;

use miden_lib::StdLibrary;
use miden_lib::agglayer::{asset_conversion_component, utils};
use miden_lib::transaction::TransactionKernel;
use miden_objects::Felt;
use miden_objects::assembly::{Assembler, DefaultSourceManager};
use miden_processor::fast::{ExecutionOutput, FastProcessor};
use miden_processor::{AdviceInputs, DefaultHost, ExecutionError, Program, StackInputs};
use primitive_types::U256;

/// Convert a Vec<Felt> to a U256
fn felts_to_u256(felts: Vec<Felt>) -> U256 {
    let bytes = utils::felts_to_u256_bytes(felts);
    U256::from_little_endian(&bytes)
}

/// Convert the top 8 u32 values from the execution stack to a U256
fn stack_to_u256(exec_output: &ExecutionOutput) -> U256 {
    let felts: Vec<Felt> = exec_output.stack[0..8].to_vec();
    felts_to_u256(felts)
}

/// Execute a program with default host
async fn execute_program_with_default_host(
    program: Program,
) -> Result<ExecutionOutput, ExecutionError> {
    let mut host = DefaultHost::default();

    let test_lib = TransactionKernel::library();
    host.load_library(test_lib.mast_forest()).unwrap();

    let std_lib = StdLibrary::default();
    host.load_library(std_lib.mast_forest()).unwrap();

    let asset_conversion_lib = miden_lib::agglayer::asset_conversion_library();
    host.load_library(asset_conversion_lib.mast_forest()).unwrap();

    let stack_inputs = StackInputs::new(vec![]).unwrap();
    let advice_inputs = AdviceInputs::default();

    let processor = FastProcessor::new_debug(stack_inputs.as_slice(), advice_inputs);
    processor.execute(&program, &mut host).await
}

#[tokio::test]
async fn test_convert_to_u256_scaled_eth() -> anyhow::Result<()> {
    // 10 base units (base 1e6)
    let miden_amount = Felt::new(10);

    // scale to 1e18
    let target_scale = Felt::new(12);

    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        target_scale.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    let expected_result = U256::from(10000000000000u64);
    let actual_result = stack_to_u256(&exec_output);

    assert_eq!(actual_result, expected_result);

    Ok(())
}

#[tokio::test]
async fn test_convert_to_u256_scaled_large_amount() -> anyhow::Result<()> {
    // 1,000,000 base units (base 1e10)
    let miden_amount = Felt::new(1_000_000);

    // scale to base 1e18
    let scale_exponent = Felt::new(8);

    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        scale_exponent.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    assert!(exec_output.stack.len() >= 8);

    let expected_result = U256::from(100_000_000_000_000u64);
    let actual_result = stack_to_u256(&exec_output);

    assert_eq!(actual_result, expected_result);

    Ok(())
}

#[tokio::test]
async fn test_convert_felt_to_u256_scaled_hand_crafted_examples() -> anyhow::Result<()> {
    // Example 1: amount=1, target_scale=0 (no scaling)
    let miden_amount = Felt::new(1);
    let target_scale = Felt::new(0);

    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        target_scale.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Expected: amount=1, scale=0 → 1 * 10^0 = 1
    // In U256 format: [[0, 0, 0, 0], [0, 0, 0, 1]]
    let expected_result = U256::from(1u64);
    let actual_result = stack_to_u256(&exec_output);
    assert_eq!(actual_result, expected_result);

    // Example 2: amount=1, target_scale=12 (scale up by 10^12)
    let miden_amount = Felt::new(1);
    let target_scale = Felt::new(12);

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        target_scale.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Expected: amount=1, scale=12 → 1 * 10^12 = 1000000000000
    let expected_result = U256::from(1000000000000u64);
    let actual_result = stack_to_u256(&exec_output);
    assert_eq!(actual_result, expected_result);

    // Example 3: amount=5, target_scale=6 (scale up by 10^6)
    let miden_amount = Felt::new(5);
    let target_scale = Felt::new(6);

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        target_scale.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Expected: amount=5, scale=6 → 5 * 10^6 = 5000000
    let expected_result = U256::from(5000000u64);
    let actual_result = stack_to_u256(&exec_output);
    assert_eq!(actual_result, expected_result);

    // Example 4: amount=100, target_scale=18 (maximum scale)
    let miden_amount = Felt::new(100);
    let target_scale = Felt::new(18);

    let script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_felt_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        target_scale.as_int(),
        miden_amount.as_int(),
    );

    let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
        .with_debug_mode(true)
        .with_dynamic_library(StdLibrary::default())
        .unwrap()
        .with_dynamic_library(asset_conversion_lib.clone())
        .unwrap()
        .assemble_program(&script_code)
        .unwrap();

    let exec_output = execute_program_with_default_host(program).await?;

    // Expected: amount=100, scale=18 → 100 * 10^18 = 100000000000000000000
    let expected_result = U256::from_dec_str("100000000000000000000").unwrap();
    let actual_result = stack_to_u256(&exec_output);
    assert_eq!(actual_result, expected_result);

    Ok(())
}

#[test]
fn test_felts_to_u256_bytes_hand_encoded_values() {
    // Test case 1: Simple sequential values 1,2,3,4,5,6,7,8
    let limbs = vec![
        Felt::new(1),
        Felt::new(2),
        Felt::new(3),
        Felt::new(4),
        Felt::new(5),
        Felt::new(6),
        Felt::new(7),
        Felt::new(8),
    ];
    let result = utils::felts_to_u256_bytes(limbs);
    assert_eq!(result.len(), 32);

    // Verify first and last limbs are in correct positions (little-endian, reversed order)
    assert_eq!(result[0], 8); // limbs[7] = 8 in little-endian (reversed order)
    assert_eq!(result[28], 1); // limbs[0] = 1 in little-endian (reversed order)
}

#[test]
fn test_felts_to_u256_bytes_edge_cases() {
    // Test case 1: All zeros (minimum)
    let limbs = vec![Felt::new(0); 8];
    let result = utils::felts_to_u256_bytes(limbs);
    assert_eq!(result.len(), 32);
    assert!(result.iter().all(|&b| b == 0));

    // Test case 2: All max u32 values (maximum)
    let limbs = vec![Felt::new(u32::MAX as u64); 8];
    let result = utils::felts_to_u256_bytes(limbs);
    assert_eq!(result.len(), 32);
    assert!(result.iter().all(|&b| b == 255));
}
