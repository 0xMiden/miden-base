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

/// Convert the top 8 u32 values from the execution stack to a U256
fn stack_to_u256(exec_output: &ExecutionOutput) -> U256 {
    let felts: Vec<Felt> = exec_output.stack[0..8].to_vec();
    let bytes = utils::felts_to_u256_bytes(felts);
    U256::from_little_endian(&bytes)
}

/// Execute a program with default host
async fn execute_program_with_default_host(
    program: Program,
    asset_conversion_lib: miden_objects::assembly::Library,
) -> Result<ExecutionOutput, ExecutionError> {
    let mut host = DefaultHost::default();

    let test_lib = TransactionKernel::library();
    host.load_library(test_lib.mast_forest()).unwrap();

    let std_lib = StdLibrary::default();
    host.load_library(std_lib.mast_forest()).unwrap();

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
            call.::convert_to_u256_scaled
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

    let exec_output =
        execute_program_with_default_host(program, asset_conversion_lib.clone()).await?;

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
            call.::convert_to_u256_scaled
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

    let exec_output =
        execute_program_with_default_host(program, asset_conversion_lib.clone()).await?;

    assert!(exec_output.stack.len() >= 8);

    let expected_result = U256::from(100_000_000_000_000u64);
    let actual_result = stack_to_u256(&exec_output);

    assert_eq!(actual_result, expected_result);

    Ok(())
}
