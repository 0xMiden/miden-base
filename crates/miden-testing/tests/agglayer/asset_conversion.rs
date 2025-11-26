extern crate alloc;

use miden_lib::agglayer::asset_conversion_component;
use miden_lib::utils::ScriptBuilder;
use miden_objects::Felt;
use miden_objects::account::{Account, AccountStorageMode};
use miden_testing::{AccountState, Auth, MockChain};
use rand::Rng;

/// Tests USDC conversion with 1e6 scale factor.
#[tokio::test]
async fn test_convert_to_u256_scaled() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Public)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    let usdc_amount = Felt::new(1_000_000_000);
    let usdc_scale = Felt::new(1_000_000); // 1e6

    let binding = asset_conversion_component(vec![]);
    let asset_conversion_lib = binding.library();

    let tx_script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            exec.sys::truncate_stack
        end
        ",
        usdc_amount.as_int(),
        usdc_scale.as_int()
    );

    let tx_script = ScriptBuilder::new(false)
        .with_dynamically_linked_library(&asset_conversion_lib)?
        .compile_tx_script(&tx_script_code)?;

    let tx_context = mock_chain
        .build_tx_context(test_account.id(), &[], &[])?
        .tx_script(tx_script)
        .build()?;

    let executed_transaction = tx_context.execute().await?;

    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    let expected_result = 1_000_000_000u64 * 1_000_000u64;
    println!("Expected USDC result: {}", expected_result);

    Ok(())
}

/// Tests ETH conversion with 1e8 scale factor.
#[tokio::test]
async fn test_convert_to_u256_scaled_eth() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Public)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    let eth_miden_amount = Felt::new(10_000_000_000); // 10^10
    let eth_scale = Felt::new(100_000_000); // 1e8

    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();

    let tx_script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            exec.sys::truncate_stack
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

    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    let expected_result = 10_000_000_000u64 * 100_000_000u64;
    println!("Expected ETH result: {}", expected_result);

    Ok(())
}

/// Tests tiny amount conversion with 1e8 scale factor.
#[tokio::test]
async fn test_convert_to_u256_scaled_tiny_amount() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let conversion_component = asset_conversion_component(vec![]);
    let seed: [u8; 32] = builder.rng_mut().random();
    let account_builder = Account::builder(seed)
        .storage_mode(AccountStorageMode::Public)
        .with_component(conversion_component);
    let test_account =
        builder.add_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)?;

    let mock_chain = builder.build()?;

    let tiny_amount = Felt::new(1);
    let eth_scale = Felt::new(100_000_000); // 1e8

    let asset_conversion_comp = asset_conversion_component(vec![]);
    let asset_conversion_lib = asset_conversion_comp.library();

    let tx_script_code = format!(
        "
        use.std::sys
        
        begin
            push.{}.{}
            call.::convert_to_u256_scaled
            exec.sys::truncate_stack
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

    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    let expected_result = 1u64 * 100_000_000u64;
    println!("Expected tiny amount result: {}", expected_result);

    Ok(())
}
