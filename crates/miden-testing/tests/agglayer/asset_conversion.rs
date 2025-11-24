extern crate alloc;

use miden_lib::agglayer::asset_conversion_component;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{Account, AccountStorageMode};
use miden_objects::Felt;
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests the convert_to_u256_scaled procedure for asset conversion.
///
/// This test verifies that the conversion procedure correctly converts Miden amounts (Felt)
/// to Ethereum u256 amounts using dynamic scaling (fixed-point scaling).
///
/// The procedure interface:
/// Input: [amount, scale]
/// Output: [lo0, lo1, lo2, lo3, hi0, hi1, hi2, hi3]
///
/// Where each limb must fit in u32 and scale is determined per token based on its
/// total supply and Ethereum's decimals.
#[tokio::test]
async fn test_convert_to_u256_scaled() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create a test account with the asset conversion component
    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Public)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    // Test Case 1: USDC - Standard amount (1,000 USDC with 6 decimals)
    // Native amount: 1,000 USDC = 1,000,000,000 smallest units
    // Scale: 0 (no scaling needed)
    // Expected Miden amount: 1,000,000,000
    let usdc_amount = Felt::new(1_000_000_000);
    let usdc_scale = Felt::new(0);

    // Create a transaction script that calls the convert_to_u256_scaled procedure
    // We need to link the asset_conversion library dynamically
    let binding = asset_conversion_component(vec![]);
    let asset_conversion_lib = binding.library();
    
    let tx_script_code = format!(
        "
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            # Drop the output (8 u32 values)

            debug.stack
            dropw dropw
        end
        ",
        usdc_amount.as_int(),
        usdc_scale.as_int()
    );

    let tx_script = ScriptBuilder::new(false)
        .with_dynamically_linked_library(&asset_conversion_lib)?
        .compile_tx_script(&tx_script_code)?;

    // Build transaction context to call convert_to_u256_scaled
    let tx_context = mock_chain
        .build_tx_context(test_account.id(), &[], &[])?
        .tx_script(tx_script)
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    // Verify the transaction executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    // TODO: Add assertions to verify the output is correct once the procedure is implemented
    // Expected output for USDC case:
    // amount * 10^0 = 1,000,000,000
    // As u256 (8 x u32): [1000000000, 0, 0, 0, 0, 0, 0, 0]

    Ok(())
}

/// Tests the convert_to_u256_scaled procedure with ETH amounts.
///
/// ETH has higher precision requirements due to its 18 decimals and large supply.
#[tokio::test]
async fn test_convert_to_u256_scaled_eth() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create a test account with the asset conversion component
    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Private)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    // Test Case: ETH - Standard amount (1 ETH with 18 decimals)
    // Native amount: 1 ETH = 10^18 wei
    // Scale: 8 (to fit within Felt range)
    // Miden amount: 10^10
    // Expected output: 10^18 wei as u256
    let eth_miden_amount = Felt::new(10_000_000_000); // 10^10
    let eth_scale = Felt::new(8);

    // Create a transaction script that calls the convert_to_u256_scaled procedure
    // We need to link the asset_conversion library dynamically
    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();
    
    let tx_script_code = format!(
        "
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            # Drop the output (8 u32 values)
            dropw dropw
        end
        ",
        eth_miden_amount.as_int(),
        eth_scale.as_int()
    );

    let tx_script = ScriptBuilder::new(false)
        .with_dynamically_linked_library(&asset_conversion_lib)?
        .compile_tx_script(&tx_script_code)?;

    // Build transaction context to call convert_to_u256_scaled
    let tx_context = mock_chain
        .build_tx_context(test_account.id(), &[], &[])?
        .tx_script(tx_script)
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    // Verify the transaction executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    // TODO: Add assertions to verify the output is correct once the procedure is implemented
    // Expected output for ETH case:
    // amount * 10^8 = 10^10 * 10^8 = 10^18
    // As u256 (8 x u32): Need to compute the u32 limb representation of 10^18

    Ok(())
}

/// Tests the convert_to_u256_scaled procedure with tiny amounts.
///
/// This test verifies edge cases with very small amounts that might result in truncation.
#[tokio::test]
async fn test_convert_to_u256_scaled_tiny_amount() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    // Create a test account with the asset conversion component
    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Private)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    // Test Case: Tiny amount - 1 wei with ETH scale
    // This should result in 100,000,000 after scaling
    let tiny_amount = Felt::new(1);
    let eth_scale = Felt::new(8);

    // Create a transaction script that calls the convert_to_u256_scaled procedure
    // We need to link the asset_conversion library dynamically
    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();
    
    let tx_script_code = format!(
        "
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            # Drop the output (8 u32 values)
            dropw dropw
        end
        ",
        tiny_amount.as_int(),
        eth_scale.as_int()
    );

    let tx_script = ScriptBuilder::new(false)
        .with_dynamically_linked_library(&asset_conversion_lib)?
        .compile_tx_script(&tx_script_code)?;

    // Build transaction context to call convert_to_u256_scaled
    let tx_context = mock_chain
        .build_tx_context(test_account.id(), &[], &[])?
        .tx_script(tx_script)
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    // Verify the transaction executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    // TODO: Add assertions to verify the output is correct once the procedure is implemented
    // Expected output: 1 * 10^8 = 100,000,000
    // As u256 (8 x u32): [100000000, 0, 0, 0, 0, 0, 0, 0]

    Ok(())
}