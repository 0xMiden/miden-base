use alloc::vec::Vec;

use anyhow::Context;
use assert_matches::assert_matches;
use miden_block_prover::{LocalBlockProver, ProvenBlockError};
use miden_lib::testing::account_component::{IncrNonceAuthComponent, MockAccountComponent};
use miden_lib::testing::mock_account::MockAccountExt;
use miden_objects::account::delta::AccountUpdateDetails;
use miden_objects::account::{Account, AccountBuilder, AccountComponent, AccountId, StorageSlot};
use miden_objects::asset::FungibleAsset;
use miden_objects::batch::ProvenBatch;
use miden_objects::block::{BlockInputs, BlockNumber, ProposedBlock};
use miden_objects::transaction::ProvenTransactionBuilder;
use miden_objects::vm::ExecutionProof;
use miden_objects::{AccountTreeError, NullifierTreeError, Word};
use miden_tx::LocalTransactionProver;
use winterfell::Proof;

use crate::kernel_tests::block::utils::MockChainBuilderBlockExt;
use crate::{Auth, MockChain, TransactionContextBuilder};

struct WitnessTestSetup {
    stale_block_inputs: BlockInputs,
    valid_block_inputs: BlockInputs,
    batches: Vec<ProvenBatch>,
}

/// Setup for a test which returns two inputs for the same block. The valid inputs match the
/// commitments of the latest block and the stale inputs match the commitments of the latest block
/// minus 1.
fn witness_test_setup() -> anyhow::Result<WitnessTestSetup> {
    let mut builder = MockChain::builder();

    let account0 = builder.add_existing_mock_account(Auth::IncrNonce)?;
    let account1 = builder.add_existing_mock_account(Auth::IncrNonce)?;
    let account2 = builder.add_existing_mock_account(Auth::IncrNonce)?;

    let note0 = builder.add_public_p2any_note(account0.id(), [FungibleAsset::mock(100)])?;
    let note1 = builder.add_public_p2any_note(account0.id(), [FungibleAsset::mock(100)])?;
    let note2 = builder.add_public_p2any_note(account0.id(), [FungibleAsset::mock(100)])?;

    let mut chain = builder.build()?;

    let tx0 = chain.create_authenticated_notes_proven_tx(account0.id(), [note0.id()])?;
    let tx1 = chain.create_authenticated_notes_proven_tx(account1.id(), [note1.id()])?;
    let tx2 = chain.create_authenticated_notes_proven_tx(account2.id(), [note2.id()])?;

    let batch1 = chain.create_batch(vec![tx1, tx2])?;
    let batches = vec![batch1];
    let stale_block_inputs = chain.get_block_inputs(&batches).unwrap();

    let account_root0 = chain.account_tree().root();
    let nullifier_root0 = chain.nullifier_tree().root();

    // Apply the executed tx and seal a block. This invalidates the block inputs we've just fetched.
    chain.add_pending_proven_transaction(tx0);
    chain.prove_next_block().unwrap();

    let valid_block_inputs = chain.get_block_inputs(&batches).unwrap();

    // Sanity check: This test requires that the tree roots change with the last sealed block so the
    // previously fetched block inputs become invalid.
    assert_ne!(chain.account_tree().root(), account_root0);
    assert_ne!(chain.nullifier_tree().root(), nullifier_root0);

    Ok(WitnessTestSetup {
        stale_block_inputs,
        valid_block_inputs,
        batches,
    })
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
    } = witness_test_setup()?;

    // Account tree root mismatch.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using the stale account witnesses.
    let mut invalid_account_tree_block_inputs = valid_block_inputs.clone();
    *invalid_account_tree_block_inputs.account_witnesses_mut() =
        stale_block_inputs.account_witnesses().clone();

    let proposed_block0 = ProposedBlock::new(invalid_account_tree_block_inputs, batches.clone())
        .context("failed to propose block 0")?;

    let error = LocalBlockProver::new(0).prove_dummy(proposed_block0).unwrap_err();

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
    } = witness_test_setup()?;

    // Nullifier tree root mismatch.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using the stale nullifier witnesses.
    let mut invalid_nullifier_tree_block_inputs = valid_block_inputs.clone();
    *invalid_nullifier_tree_block_inputs.nullifier_witnesses_mut() =
        stale_block_inputs.nullifier_witnesses().clone();

    let proposed_block2 = ProposedBlock::new(invalid_nullifier_tree_block_inputs, batches.clone())
        .context("failed to propose block 2")?;

    let error = LocalBlockProver::new(0).prove_dummy(proposed_block2).unwrap_err();

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
    } = witness_test_setup()?;

    // Stale and current account witnesses used together.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using a single stale account witness.
    let mut stale_account_witness_block_inputs = valid_block_inputs.clone();
    let batch_account_id0 =
        batches[0].updated_accounts().next().context("failed to get updated account")?;

    *stale_account_witness_block_inputs
        .account_witnesses_mut()
        .get_mut(&batch_account_id0)
        .context("failed to get account witness")? = stale_block_inputs
        .account_witnesses_mut()
        .get_mut(&batch_account_id0)
        .context("failed to get stale account witness")?
        .clone();

    let proposed_block1 = ProposedBlock::new(stale_account_witness_block_inputs, batches.clone())
        .context("failed to propose block 1")?;

    let error = LocalBlockProver::new(0).prove_dummy(proposed_block1).unwrap_err();

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
    } = witness_test_setup()?;

    // Stale and current nullifier witnesses used together.
    // --------------------------------------------------------------------------------------------

    // Make the block inputs invalid by using a single stale nullifier witnesses.
    let mut invalid_nullifier_witness_block_inputs = valid_block_inputs.clone();
    let batch_nullifier0 = batches[0]
        .created_nullifiers()
        .next()
        .context("failed to get created nullifier")?;

    *invalid_nullifier_witness_block_inputs
        .nullifier_witnesses_mut()
        .get_mut(&batch_nullifier0)
        .context("failed to get nullifier witness")? = stale_block_inputs
        .nullifier_witnesses_mut()
        .get_mut(&batch_nullifier0)
        .context("failed to get stale nullifier witness")?
        .clone();

    let proposed_block3 = ProposedBlock::new(invalid_nullifier_witness_block_inputs, batches)
        .context("failed to propose block 3")?;

    let error = LocalBlockProver::new(0).prove_dummy(proposed_block3).unwrap_err();

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

    let mut builder = MockChain::builder();

    let auth_component: AccountComponent = IncrNonceAuthComponent.into();

    let account = AccountBuilder::new([5; 32])
        .with_auth_component(auth_component.clone())
        .with_component(MockAccountComponent::with_slots(vec![StorageSlot::Value(Word::from(
            [5u32; 4],
        ))]))
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
    assert_eq!(account.initial_commitment(), Word::empty());

    let existing_account = Account::mock(existing_id.into(), auth_component);
    builder.add_account(existing_account.clone())?;
    let mock_chain = builder.build()?;

    // Execute the account-creating transaction.
    // --------------------------------------------------------------------------------------------

    let tx_inputs = mock_chain.get_transaction_inputs(&account, &[], &[])?;
    let tx_context = TransactionContextBuilder::new(account).tx_inputs(tx_inputs).build()?;
    let tx = tx_context.execute_blocking().context("failed to execute account creating tx")?;
    let tx = LocalTransactionProver::default().prove_dummy(tx)?;

    let batch = mock_chain.create_batch(vec![tx])?;
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

    let err = LocalBlockProver::new(0).prove_dummy(block).unwrap_err();

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
    let mock_chain = MockChain::new();
    let account = AccountBuilder::new([5; 32])
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_slots(vec![StorageSlot::Value(Word::from(
            [5u32; 4],
        ))]))
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
                Word::empty(),
                Word::from(final_state_comm),
                Word::empty(),
                genesis_block.block_num(),
                genesis_block.commitment(),
                FungibleAsset::mock(500).unwrap_fungible(),
                BlockNumber::from(u32::MAX),
                ExecutionProof::new(Proof::new_dummy(), Default::default()),
            )
            .account_update_details(AccountUpdateDetails::Private)
            .build()
            .context("failed to build proven transaction")
            .unwrap()
        });

    // Build a batch from these transactions and attempt to prove a block.
    // --------------------------------------------------------------------------------------------

    let batch = mock_chain.create_batch(vec![tx0, tx1])?;
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

    assert_eq!(witness0.state_commitment(), Word::empty());
    assert_eq!(witness1.state_commitment(), Word::empty());

    let block = mock_chain.propose_block(batches).context("failed to propose block")?;

    let err = LocalBlockProver::new(0).prove_dummy(block).unwrap_err();

    // This should fail when we try to _track_ the same two prefixes in the partial tree.
    assert_matches!(
        err,
        ProvenBlockError::AccountWitnessTracking {
            source: AccountTreeError::DuplicateIdPrefix { duplicate_prefix }
        } if duplicate_prefix == id0.prefix()
    );

    Ok(())
}
