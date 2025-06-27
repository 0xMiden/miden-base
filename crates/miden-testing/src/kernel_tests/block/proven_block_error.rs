use alloc::vec::Vec;

use anyhow::Context;
use assert_matches::assert_matches;
use miden_block_prover::{LocalBlockProver, ProvenBlockError};
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    AccountTreeError, Digest, EMPTY_WORD, Felt, FieldElement, NullifierTreeError,
    account::{Account, AccountBuilder, AccountId, StorageSlot, delta::AccountUpdateDetails},
    batch::ProvenBatch,
    block::{BlockInputs, BlockNumber, ProposedBlock},
    testing::account_component::AccountMockComponent,
    transaction::{ProvenTransaction, ProvenTransactionBuilder, TransactionScript},
    vm::ExecutionProof,
};
use winterfell::Proof;

use super::utils::{
    TestSetup, generate_batch, generate_executed_tx_with_authenticated_notes,
    generate_tracked_note, setup_chain,
};
use crate::{MockChain, ProvenTransactionExt, TransactionContextBuilder};

struct WitnessTestSetup {
    stale_block_inputs: BlockInputs,
    valid_block_inputs: BlockInputs,
    batches: Vec<ProvenBatch>,
}

/// Setup for a test which returns two inputs for the same block. The valid inputs match the
/// commitments of the latest block and the stale inputs match the commitments of the latest block
/// minus 1.
fn witness_test_setup() -> WitnessTestSetup {
    let TestSetup { mut chain, mut accounts, mut txs, .. } = setup_chain(4);

    let account0 = accounts.remove(&0).unwrap();
    let account1 = accounts.remove(&1).unwrap();

    let note = generate_tracked_note(&mut chain, account1.id(), account0.id());
    // Add note to chain.
    chain.prove_next_block().unwrap();

    let tx0 = generate_executed_tx_with_authenticated_notes(&chain, account0.id(), &[note.id()]);
    let tx1 = txs.remove(&1).unwrap();
    let tx2 = txs.remove(&2).unwrap();

    let batch1 = generate_batch(&mut chain, vec![tx1, tx2]);
    let batches = vec![batch1];
    let stale_block_inputs = chain.get_block_inputs(&batches).unwrap();

    let account_root0 = chain.account_tree().root();
    let nullifier_root0 = chain.nullifier_tree().root();

    // Apply the executed tx and seal a block. This invalidates the block inputs we've just fetched.
    chain.add_pending_executed_transaction(&tx0).unwrap();
    chain.prove_next_block().unwrap();

    let valid_block_inputs = chain.get_block_inputs(&batches).unwrap();

    // Sanity check: This test requires that the tree roots change with the last sealed block so the
    // previously fetched block inputs become invalid.
    assert_ne!(chain.account_tree().root(), account_root0);
    assert_ne!(chain.nullifier_tree().root(), nullifier_root0);

    WitnessTestSetup {
        stale_block_inputs,
        valid_block_inputs,
        batches,
    }
}

/// Tests that a proven block cannot be built if witnesses from a stale account tree are used
/// (i.e. an account tree whose root is not in the previous block header).
#[test]
fn proven_block_fails_on_stale_account_witnesses() -> anyhow::Result<()> {
    // Setup test with stale and valid block inputs.
    // --------------------------------------------------------------------------------------------

    let WitnessTestSetup {
        stale_block_inputs,
        valid_block_inputs,
        batches,
    } = witness_test_setup();

    // Account tree root mismatch.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using the stale account witnesses.
    let mut invalid_account_tree_block_inputs = valid_block_inputs.clone();
    *invalid_account_tree_block_inputs.account_witnesses_mut() =
        stale_block_inputs.account_witnesses().clone();

    let proposed_block0 = ProposedBlock::new(invalid_account_tree_block_inputs, batches.clone())
        .context("failed to propose block 0")?;

    let error = LocalBlockProver::new(0)
        .prove_without_batch_verification(proposed_block0)
        .unwrap_err();

    assert_matches!(
        error,
        ProvenBlockError::StaleAccountTreeRoot {
            prev_block_account_root,
            ..
        } if prev_block_account_root == valid_block_inputs.prev_block_header().account_root()
    );

    Ok(())
}

/// Tests that a proven block cannot be built if witnesses from a stale nullifier tree are used
/// (i.e. a nullifier tree whose root is not in the previous block header).
#[test]
fn proven_block_fails_on_stale_nullifier_witnesses() -> anyhow::Result<()> {
    // Setup test with stale and valid block inputs.
    // --------------------------------------------------------------------------------------------

    let WitnessTestSetup {
        stale_block_inputs,
        valid_block_inputs,
        batches,
    } = witness_test_setup();

    // Nullifier tree root mismatch.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using the stale nullifier witnesses.
    let mut invalid_nullifier_tree_block_inputs = valid_block_inputs.clone();
    *invalid_nullifier_tree_block_inputs.nullifier_witnesses_mut() =
        stale_block_inputs.nullifier_witnesses().clone();

    let proposed_block2 = ProposedBlock::new(invalid_nullifier_tree_block_inputs, batches.clone())
        .context("failed to propose block 2")?;

    let error = LocalBlockProver::new(0)
        .prove_without_batch_verification(proposed_block2)
        .unwrap_err();

    assert_matches!(
        error,
        ProvenBlockError::StaleNullifierTreeRoot {
          prev_block_nullifier_root,
          ..
        } if prev_block_nullifier_root == valid_block_inputs.prev_block_header().nullifier_root()
    );

    Ok(())
}

/// Tests that a proven block cannot be built if both witnesses from a stale account tree and from
/// the current account tree are used which results in different account tree roots.
#[test]
fn proven_block_fails_on_account_tree_root_mismatch() -> anyhow::Result<()> {
    // Setup test with stale and valid block inputs.
    // --------------------------------------------------------------------------------------------

    let WitnessTestSetup {
        mut stale_block_inputs,
        valid_block_inputs,
        batches,
    } = witness_test_setup();

    // Stale and current account witnesses used together.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using a single stale account witness.
    let mut stale_account_witness_block_inputs = valid_block_inputs.clone();
    let batch_account_id0 = batches[0].updated_accounts().next().unwrap();

    *stale_account_witness_block_inputs
        .account_witnesses_mut()
        .get_mut(&batch_account_id0)
        .unwrap() = stale_block_inputs
        .account_witnesses_mut()
        .get_mut(&batch_account_id0)
        .unwrap()
        .clone();

    let proposed_block1 = ProposedBlock::new(stale_account_witness_block_inputs, batches.clone())
        .context("failed to propose block 1")?;

    let error = LocalBlockProver::new(0)
        .prove_without_batch_verification(proposed_block1)
        .unwrap_err();

    assert_matches!(
        error,
        ProvenBlockError::AccountWitnessTracking {
            source: AccountTreeError::TreeRootConflict { .. },
            ..
        }
    );

    Ok(())
}

/// Tests that a proven block cannot be built if both witnesses from a stale nullifier tree and from
/// the current nullifier tree are used which results in different nullifier tree roots.
#[test]
fn proven_block_fails_on_nullifier_tree_root_mismatch() -> anyhow::Result<()> {
    // Setup test with stale and valid block inputs.
    // --------------------------------------------------------------------------------------------

    let WitnessTestSetup {
        mut stale_block_inputs,
        valid_block_inputs,
        batches,
    } = witness_test_setup();

    // Stale and current nullifier witnesses used together.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using a single stale nullifier witnesses.
    let mut invalid_nullifier_witness_block_inputs = valid_block_inputs.clone();
    let batch_nullifier0 = batches[0].created_nullifiers().next().unwrap();

    *invalid_nullifier_witness_block_inputs
        .nullifier_witnesses_mut()
        .get_mut(&batch_nullifier0)
        .unwrap() = stale_block_inputs
        .nullifier_witnesses_mut()
        .get_mut(&batch_nullifier0)
        .unwrap()
        .clone();

    let proposed_block3 = ProposedBlock::new(invalid_nullifier_witness_block_inputs, batches)
        .context("failed to propose block 3")?;

    let error = LocalBlockProver::new(0)
        .prove_without_batch_verification(proposed_block3)
        .unwrap_err();

    assert_matches!(
        error,
        ProvenBlockError::NullifierWitnessRootMismatch(NullifierTreeError::TreeRootConflict(_))
    );

    Ok(())
}

/// Tests that creating an account when an existing account with the same account ID prefix exists,
/// results in an error.
#[test]
fn proven_block_fails_on_creating_account_with_existing_account_id_prefix() -> anyhow::Result<()> {
    // Construct a new account.
    // --------------------------------------------------------------------------------------------

    let mut mock_chain = MockChain::new();
    let (account, seed) = AccountBuilder::new([5; 32])
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::testing_assembler(),
                vec![StorageSlot::Value([5u32.into(); 4])],
            )
            .unwrap(),
        )
        .build()
        .context("failed to build account")?;

    let new_id = account.id();

    // Construct a second account whose ID matches the prefix of the first and insert it into the
    // chain, as if that account already existed. That way we can check if the block prover errors
    // when we attempt to create the first account.
    // --------------------------------------------------------------------------------------------

    // Set some bits on the hash part of the suffix to make the account id distinct from the
    // original one, but their prefix is still the same.
    let existing_id = AccountId::try_from(u128::from(new_id) | 0xffff00)
        .context("failed to convert account ID")?;

    assert_eq!(
        new_id.prefix(),
        existing_id.prefix(),
        "test requires that prefixes are the same"
    );
    assert_ne!(
        new_id.suffix(),
        existing_id.suffix(),
        "test should work if suffixes are different, so we want to ensure it"
    );
    assert_eq!(account.init_commitment(), miden_objects::Digest::from(EMPTY_WORD));

    let existing_account =
        Account::mock(existing_id.into(), Felt::ZERO, TransactionKernel::testing_assembler());
    mock_chain.add_pending_account(existing_account.clone());
    mock_chain.prove_next_block()?;

    // Execute the account-creating transaction.
    // --------------------------------------------------------------------------------------------

    // transaction code which only increases the nonce to make the transaction non-empty
    let default_tx_code = "
        use.miden::account 
        
        begin 
            push.1 call.account::incr_nonce drop
        end";
    let default_tx_script =
        TransactionScript::compile(default_tx_code, TransactionKernel::testing_assembler())
            .context("failed to compile the transaction script")?;

    let tx_inputs = mock_chain.get_transaction_inputs(account.clone(), Some(seed), &[], &[])?;
    let tx_context = TransactionContextBuilder::new(account)
        .tx_script(default_tx_script)
        .account_seed(Some(seed))
        .tx_inputs(tx_inputs)
        .build();
    let tx = tx_context.execute().context("failed to execute account creating tx")?;
    let tx = ProvenTransaction::from_executed_transaction_mocked(tx);

    let batch = generate_batch(&mut mock_chain, vec![tx]);
    let batches = [batch];

    let block_inputs = mock_chain.get_block_inputs(batches.iter())?;
    // Sanity check: The mock chain account tree root should match the previous block header's
    // account tree root.
    assert_eq!(
        mock_chain.account_tree().root(),
        block_inputs.prev_block_header().account_root()
    );
    assert_eq!(mock_chain.account_tree().num_accounts(), 1);

    // Sanity check: The block inputs should contain an account witness whose ID matches the
    // existing ID.
    assert_eq!(block_inputs.account_witnesses().len(), 1);
    let witness = block_inputs
        .account_witnesses()
        .get(&new_id)
        .context("block inputs did not contain witness for id")?;

    // The witness should be for the **existing** account, because that's the one that exists in
    // the tree and is therefore in the same SMT leaf that we would insert the new ID into.
    assert_eq!(witness.id(), existing_id);
    assert_eq!(witness.state_commitment(), existing_account.commitment());

    let block = mock_chain.propose_block(batches).context("failed to propose block")?;

    let err = LocalBlockProver::new(0).prove_without_batch_verification(block).unwrap_err();

    // This should fail when we try to _insert_ the same two prefixes into the partial tree.
    assert_matches!(
        err,
        ProvenBlockError::AccountIdPrefixDuplicate {
            source: AccountTreeError::DuplicateIdPrefix { duplicate_prefix }
        } if duplicate_prefix == new_id.prefix()
    );

    Ok(())
}

/// Tests that creating two accounts in the same block whose ID prefixes match, results in an error.
#[test]
fn proven_block_fails_on_creating_account_with_duplicate_account_id_prefix() -> anyhow::Result<()> {
    // Construct a new account.
    // --------------------------------------------------------------------------------------------

    let mut mock_chain = MockChain::new();
    let (account, _) = AccountBuilder::new([5; 32])
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::testing_assembler(),
                vec![StorageSlot::Value([5u32.into(); 4])],
            )
            .unwrap(),
        )
        .build()
        .context("failed to build account")?;

    let id0 = account.id();

    // Construct a second account whose ID matches the prefix of the first.
    // --------------------------------------------------------------------------------------------

    // Set some bits on the hash part of the suffix to make the account id distinct from the
    // original one, but their prefix is still the same.
    let id1 =
        AccountId::try_from(u128::from(id0) | 0xffff00).context("failed to convert account ID")?;

    assert_eq!(id0.prefix(), id1.prefix(), "test requires that prefixes are the same");
    assert_ne!(
        id0.suffix(),
        id1.suffix(),
        "test should work if suffixes are different, so we want to ensure it"
    );

    // Build two mocked proven transactions, each of which creates a new account and both share the
    // same ID prefix but not the suffix.
    // --------------------------------------------------------------------------------------------

    let genesis_block = mock_chain.block_header(0);

    let [tx0, tx1] =
        [(id0, [0, 0, 0, 1u32]), (id1, [0, 0, 0, 2u32])].map(|(id, final_state_comm)| {
            ProvenTransactionBuilder::new(
                id,
                Digest::default(),
                Digest::from(final_state_comm),
                Digest::default(),
                genesis_block.block_num(),
                genesis_block.commitment(),
                BlockNumber::from(u32::MAX),
                ExecutionProof::new(Proof::new_dummy(), Default::default()),
            )
            .account_update_details(AccountUpdateDetails::Private)
            .build()
            .unwrap()
        });

    // Build a batch from these transactions and attempt to prove a block.
    // --------------------------------------------------------------------------------------------

    let batch = generate_batch(&mut mock_chain, vec![tx0, tx1]);
    let batches = [batch];

    // Sanity check: The block inputs should contain two account witnesses that point to the same
    // empty entry.
    let block_inputs = mock_chain.get_block_inputs(batches.iter())?;
    assert_eq!(block_inputs.account_witnesses().len(), 2);
    let witness0 = block_inputs
        .account_witnesses()
        .get(&id0)
        .context("block inputs did not contain witness for id0")?;
    let witness1 = block_inputs
        .account_witnesses()
        .get(&id1)
        .context("block inputs did not contain witness for id1")?;
    assert_eq!(witness0.id(), id0);
    assert_eq!(witness1.id(), id1);

    assert_eq!(witness0.state_commitment(), Digest::default());
    assert_eq!(witness1.state_commitment(), Digest::default());

    let block = mock_chain.propose_block(batches).context("failed to propose block")?;

    let err = LocalBlockProver::new(0).prove_without_batch_verification(block).unwrap_err();

    // This should fail when we try to _track_ the same two prefixes in the partial tree.
    assert_matches!(
        err,
        ProvenBlockError::AccountWitnessTracking {
            source: AccountTreeError::DuplicateIdPrefix { duplicate_prefix }
        } if duplicate_prefix == id0.prefix()
    );

    Ok(())
}
