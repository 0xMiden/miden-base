use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::AccountBuilder, testing::account_component::AccountMockComponent,
    transaction::TransactionScript,
};

use crate::MockChain;

// ACCOUNT DELTA TESTS
// ================================================================================================

#[test]
fn test_delta_nonce() -> anyhow::Result<()> {
    let storage_slots = vec![];
    let account = AccountBuilder::new([8; 32])
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::testing_assembler(),
                storage_slots,
            )
            .context("failed to assemble account mock component")?,
        )
        .build_existing()
        .context("failed to build account")?;

    let account_id = account.id();
    let mock_chain = MockChain::with_accounts(&[account]);

    let tx_context_builder = mock_chain
        .build_tx_context(account_id, &[], &[])
        .tx_script(authenticate_mock_account_tx_script());
    let executed_tx =
        tx_context_builder.build().execute().context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce(), Some(miden_objects::Felt::new(3)));

    Ok(())
}

fn authenticate_mock_account_tx_script() -> TransactionScript {
    let code = "
      use.test::account
      use.miden::tx

      begin
          padw padw padw
          push.0.0.0.3
          # => [3, pad(15)]

          call.account::incr_nonce
          # => [pad(16)]

          dropw dropw dropw dropw
      end
      ";

    TransactionScript::compile(code, [], TransactionKernel::testing_assembler_with_mock_account())
        .unwrap()
}
