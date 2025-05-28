use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    account::AccountBuilder, testing::account_component::AccountMockComponent,
    transaction::TransactionScript,
};

use crate::MockChain;

// ACCOUNT DELTA TESTS
// ================================================================================================

/// Tests that incrementing the nonce by 3 twice results in a nonce delta of 6.
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

    let code = "
      use.test::account
      use.miden::tx

      begin
          repeat.2
            padw padw padw
            push.0.0.0.3
            # => [3, pad(15)]

            call.account::incr_nonce
            # => [pad(16)]

            dropw dropw dropw dropw
          end
      end
    ";
    let tx_script = TransactionScript::compile(
        code,
        [],
        TransactionKernel::testing_assembler_with_mock_account(),
    )
    .context("failed to compile tx script")?;

    let tx_context_builder = mock_chain.build_tx_context(account_id, &[], &[]).tx_script(tx_script);
    let executed_tx =
        tx_context_builder.build().execute().context("failed to execute transaction")?;

    assert_eq!(executed_tx.account_delta().nonce(), Some(miden_objects::Felt::new(6)));

    Ok(())
}
