use std::{collections::BTreeMap, vec::Vec};

use anyhow::Context;
use miden_block_prover::LocalBlockProver;
use miden_objects::{
    MIN_PROOF_SECURITY_LEVEL,
    batch::BatchNoteTree,
    block::{AccountTree, BlockInputs, BlockNoteIndex, BlockNoteTree, ProposedBlock},
    crypto::merkle::Smt,
    transaction::{InputNoteCommitment, OutputNote},
};
use rand::Rng;

use super::utils::{
    TestSetup, generate_batch, generate_executed_tx_with_authenticated_notes, generate_output_note,
    generate_tracked_note, generate_tx_with_authenticated_notes,
    generate_tx_with_unauthenticated_notes, generate_untracked_note_with_output_note, setup_chain,
};

/// Tests the outputs of a proven block with transactions that consume notes, create output notes
/// and modify the account's state.
#[test]
fn proven_block_success() -> anyhow::Result<()> {
    // Setup test with notes that produce output notes, in order to test the block note tree root
    // computation.
    // --------------------------------------------------------------------------------------------

    let TestSetup { mut chain, mut accounts, .. } = setup_chain(4);

    let account0 = accounts.remove(&0).unwrap();
    let account1 = accounts.remove(&1).unwrap();
    let account2 = accounts.remove(&2).unwrap();
    let account3 = accounts.remove(&3).unwrap();

    let output_note0 = generate_output_note(account0.id(), [0; 32]);
    let output_note1 = generate_output_note(account1.id(), [1; 32]);
    let output_note2 = generate_output_note(account2.id(), [2; 32]);
    let output_note3 = generate_output_note(account3.id(), [3; 32]);

    let input_note0 = generate_untracked_note_with_output_note(account0.id(), output_note0);
    let input_note1 = generate_untracked_note_with_output_note(account1.id(), output_note1);
    let input_note2 = generate_untracked_note_with_output_note(account2.id(), output_note2);
    let input_note3 = generate_untracked_note_with_output_note(account3.id(), output_note3);

    // Add input notes to chain so we can consume them.
    chain.add_pending_note(OutputNote::Full(input_note0.clone()));
    chain.add_pending_note(OutputNote::Full(input_note1.clone()));
    chain.add_pending_note(OutputNote::Full(input_note2.clone()));
    chain.add_pending_note(OutputNote::Full(input_note3.clone()));
    chain.prove_next_block();

    let tx0 = generate_tx_with_authenticated_notes(&mut chain, account0.id(), &[input_note0.id()]);
    let tx1 = generate_tx_with_authenticated_notes(&mut chain, account1.id(), &[input_note1.id()]);
    let tx2 = generate_tx_with_authenticated_notes(&mut chain, account2.id(), &[input_note2.id()]);
    let tx3 = generate_tx_with_authenticated_notes(&mut chain, account3.id(), &[input_note3.id()]);

    let batch0 = generate_batch(&mut chain, [tx0.clone(), tx1.clone()].to_vec());
    let batch1 = generate_batch(&mut chain, [tx2.clone(), tx3.clone()].to_vec());

    // Sanity check: Batches should have two output notes each.
    assert_eq!(batch0.output_notes().len(), 2);
    assert_eq!(batch1.output_notes().len(), 2);

    let proposed_block = chain
        .propose_block([batch0.clone(), batch1.clone()])
        .context("failed to propose block")?;

    // Compute expected block note tree.
    // --------------------------------------------------------------------------------------------

    let batch0_iter = batch0
        .output_notes()
        .iter()
        .enumerate()
        .map(|(note_idx_in_batch, note)| (0, note_idx_in_batch, note));
    let batch1_iter = batch1
        .output_notes()
        .iter()
        .enumerate()
        .map(|(note_idx_in_batch, note)| (1, note_idx_in_batch, note));

    let expected_block_note_tree = BlockNoteTree::with_entries(batch0_iter.chain(batch1_iter).map(
        |(batch_idx, note_idx_in_batch, note)| {
            (
                BlockNoteIndex::new(batch_idx, note_idx_in_batch).unwrap(),
                note.id(),
                *note.metadata(),
            )
        },
    ))
    .unwrap();

    // Compute expected nullifier root on the full SMT.
    // --------------------------------------------------------------------------------------------

    let mut expected_nullifier_tree = chain.nullifier_tree().clone();
    for nullifier in proposed_block.created_nullifiers().keys() {
        expected_nullifier_tree
            .mark_spent(*nullifier, proposed_block.block_num())
            .context("failed to mark nullifier as spent")?;
    }

    // Compute expected account root on the full account tree.
    // --------------------------------------------------------------------------------------------

    let mut expected_account_tree = chain.account_tree().clone();
    for (account_id, witness) in proposed_block.updated_accounts() {
        expected_account_tree
            .insert(*account_id, witness.final_state_commitment())
            .context("failed to insert account id into account tree")?;
    }

    // Prove block.
    // --------------------------------------------------------------------------------------------

    let proven_block = LocalBlockProver::new(MIN_PROOF_SECURITY_LEVEL)
        .prove_without_batch_verification(proposed_block)
        .context("failed to prove proposed block")?;

    // Check tree/chain commitments against expected values.
    // --------------------------------------------------------------------------------------------

    assert_eq!(proven_block.header().nullifier_root(), expected_nullifier_tree.root());
    assert_eq!(proven_block.header().account_root(), expected_account_tree.root());

    // The Mmr in MockChain adds a new block after it is sealed, so at this point the chain contains
    // block2 and has length 3.
    // This means the chain commitment of the mock chain must match the chain commitment of the
    // PartialBlockchain with chain length 2 when the prev block (block2) is added.
    assert_eq!(
        proven_block.header().chain_commitment(),
        chain.blockchain().peaks().hash_peaks()
    );

    assert_eq!(proven_block.header().note_root(), expected_block_note_tree.root());
    // Assert that the block note tree can be reconstructed.
    assert_eq!(proven_block.build_output_note_tree(), expected_block_note_tree);

    // Check input notes / nullifiers.
    // --------------------------------------------------------------------------------------------

    assert_eq!(proven_block.created_nullifiers().len(), 4);
    assert!(proven_block.created_nullifiers().contains(&input_note0.nullifier()));
    assert!(proven_block.created_nullifiers().contains(&input_note1.nullifier()));
    assert!(proven_block.created_nullifiers().contains(&input_note2.nullifier()));
    assert!(proven_block.created_nullifiers().contains(&input_note3.nullifier()));

    // Check output notes.
    // --------------------------------------------------------------------------------------------

    assert_eq!(proven_block.output_note_batches().len(), 2);
    assert_eq!(
        proven_block.output_note_batches()[0],
        batch0.output_notes().iter().cloned().enumerate().collect::<Vec<_>>()
    );
    assert_eq!(
        proven_block.output_note_batches()[1],
        batch1.output_notes().iter().cloned().enumerate().collect::<Vec<_>>()
    );

    // Check account updates.
    // --------------------------------------------------------------------------------------------

    // The block-level account updates should be the same as the ones on transaction-level.
    for (tx, batch) in [(&tx0, &batch0), (&tx1, &batch0), (&tx2, &batch1), (&tx3, &batch1)] {
        let updated_account = tx.account_id();
        let block_account_update = proven_block
            .updated_accounts()
            .iter()
            .find(|update| update.account_id() == updated_account)
            .expect("account should have been updated in the block");

        assert_eq!(
            block_account_update.final_state_commitment(),
            batch.account_updates().get(&updated_account).unwrap().final_state_commitment()
        );
    }

    Ok(())
}

/// Tests that an unauthenticated note is erased when it is created in the same block.
///
/// The high level test setup is that there are four transactions split in two batches:
/// tx0 (batch0): consume note0 -> create output_note0.
/// tx1 (batch1): consume output_note0.
/// tx2 (batch0): consume note2 -> create output_note2.
/// tx3 (batch0): consume note3 -> create output_note3.
///
/// The expected result is that output_note0 is erased from the set of output notes of the block.
///
/// We also test that the batch note tree containing the output generating transactions is a subtree
/// of the subtree of the overall block note tree computed from the block's output notes.
#[test]
fn proven_block_erasing_unauthenticated_notes() -> anyhow::Result<()> {
    let TestSetup { mut chain, mut accounts, .. } = setup_chain(4);
    let account0 = accounts.remove(&0).unwrap();
    let account1 = accounts.remove(&1).unwrap();
    let account2 = accounts.remove(&2).unwrap();
    let account3 = accounts.remove(&3).unwrap();

    // Use an Rng to randomize the note IDs and therefore their position in the output note batches.
    // This is useful to test that the block note tree is correctly computed no matter at what index
    // the erased note ends up in.
    let mut rng = rand::rng();
    let output_note0 = generate_output_note(account0.id(), rng.random());
    let output_note2 = generate_output_note(account2.id(), rng.random());
    let output_note3 = generate_output_note(account3.id(), rng.random());

    // Create notes that, when consumed, will create the above corresponding output notes.
    let note0 = generate_untracked_note_with_output_note(account0.id(), output_note0.clone());
    let note2 = generate_untracked_note_with_output_note(account2.id(), output_note2.clone());
    let note3 = generate_untracked_note_with_output_note(account3.id(), output_note3.clone());

    // Add note{0,2,3} to the chain so we can consume them.
    chain.add_pending_note(OutputNote::Full(note0.clone()));
    chain.add_pending_note(OutputNote::Full(note2.clone()));
    chain.add_pending_note(OutputNote::Full(note3.clone()));
    chain.prove_next_block();

    let tx0 = generate_tx_with_authenticated_notes(&mut chain, account0.id(), &[note0.id()]);
    let tx1 =
        generate_tx_with_unauthenticated_notes(&mut chain, account1.id(), &[output_note0.clone()]);
    let tx2 = generate_tx_with_authenticated_notes(&mut chain, account2.id(), &[note2.id()]);
    let tx3 = generate_tx_with_authenticated_notes(&mut chain, account3.id(), &[note3.id()]);

    assert_eq!(tx0.input_notes().num_notes(), 1);
    assert_eq!(tx0.output_notes().num_notes(), 1);
    assert_eq!(tx1.output_notes().num_notes(), 0);
    // The unauthenticated note is an input note of the tx.
    assert_eq!(tx1.input_notes().num_notes(), 1);

    // Sanity check: The input note of tx0 and output note of tx1 should be the same.
    assert_eq!(
        tx0.output_notes().get_note(0).id(),
        tx1.input_notes().get_note(0).header().unwrap().id()
    );

    let batch0 = generate_batch(&mut chain, vec![tx2.clone(), tx0.clone(), tx3.clone()]);
    let batch1 = generate_batch(&mut chain, vec![tx1.clone()]);

    // Sanity check: The batches and contained transactions should have the same input notes (sorted
    // by nullifier).
    let mut expected_input_notes: Vec<_> = tx2
        .input_notes()
        .iter()
        .chain(tx0.input_notes())
        .chain(tx3.input_notes())
        .cloned()
        .collect();
    expected_input_notes.sort_by_key(InputNoteCommitment::nullifier);

    assert_eq!(batch0.input_notes().clone().into_vec(), expected_input_notes);
    assert_eq!(batch1.input_notes(), tx1.input_notes());

    let batches = [batch0.clone(), batch1];
    // This block will use block2 as the reference block.
    let mut block_inputs = chain.get_block_inputs(&batches);

    // Remove the nullifier witness for output_note0 which will be erased, to check that the
    // proposed block does not _require_ nullifier witnesses for erased notes.
    block_inputs
        .nullifier_witnesses_mut()
        .remove(&output_note0.nullifier())
        .unwrap();

    let proposed_block = ProposedBlock::new(block_inputs.clone(), batches.to_vec())
        .context("failed to build proposed block")?;

    // The output note should have been erased, so we expect only the nullifiers of note0, note2 and
    // note3 to be created.
    assert_eq!(proposed_block.created_nullifiers().len(), 3);
    assert!(proposed_block.created_nullifiers().contains_key(&note0.nullifier()));
    assert!(proposed_block.created_nullifiers().contains_key(&note2.nullifier()));
    assert!(proposed_block.created_nullifiers().contains_key(&note3.nullifier()));

    // There are two batches in the block.
    assert_eq!(proposed_block.output_note_batches().len(), 2);
    // The second batch does not create any notes.
    assert!(proposed_block.output_note_batches()[1].is_empty());

    // Construct the expected output notes by collecting all output notes from all transactions in
    // batch0. We use a BTreeMap to sort by NoteId and then map each note to its index in this
    // sorted list.
    let mut expected_output_notes_batch0: Vec<_> = tx2
        .output_notes()
        .iter()
        .chain(tx0.output_notes().iter())
        .chain(tx3.output_notes().iter())
        .cloned()
        .map(|note| (note.id(), note))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .enumerate()
        .map(|(note_idx, (_, note))| (note_idx, note))
        .collect();

    // Find and remove the erased note from the expected output notes.
    let erased_note_idx = expected_output_notes_batch0
        .iter()
        .find_map(|(idx, note)| (note.id() == output_note0.id()).then_some(idx))
        .copied()
        .unwrap();
    expected_output_notes_batch0.remove(erased_note_idx as usize);

    let output_notes_batch0 = &proposed_block.output_note_batches()[0];
    // The first batch creates three notes, one of which is erased, so we expect 2 notes in the
    // output note batch.
    assert_eq!(output_notes_batch0.len(), 2);
    assert_eq!(output_notes_batch0, &expected_output_notes_batch0);

    let proven_block = LocalBlockProver::new(0)
        .prove_without_batch_verification(proposed_block)
        .context("failed to prove block")?;
    let actual_block_note_tree = proven_block.build_output_note_tree();

    // Remove the erased note to get the expected batch note tree.
    let mut batch_tree = BatchNoteTree::with_contiguous_leaves(
        batch0.output_notes().iter().map(|note| (note.id(), note.metadata())),
    )
    .unwrap();
    batch_tree.remove(erased_note_idx as u64).unwrap();

    let mut expected_block_note_tree = BlockNoteTree::empty();
    expected_block_note_tree.insert_batch_note_subtree(0, batch_tree).unwrap();

    assert_eq!(expected_block_note_tree.root(), actual_block_note_tree.root());

    Ok(())
}

/// Tests that we can build empty blocks.
#[test]
fn proven_block_succeeds_with_empty_batches() -> anyhow::Result<()> {
    // Setup a chain with a non-empty nullifier tree by consuming some notes.
    // --------------------------------------------------------------------------------------------

    let TestSetup { mut chain, mut accounts, .. } = setup_chain(2);

    let account0 = accounts.remove(&0).unwrap();
    let account1 = accounts.remove(&1).unwrap();

    // Add notes to the chain we can consume.
    let note0 = generate_tracked_note(&mut chain, account1.id(), account0.id());
    let note1 = generate_tracked_note(&mut chain, account0.id(), account1.id());
    chain.prove_next_block();

    let tx0 = generate_executed_tx_with_authenticated_notes(&chain, account0.id(), &[note0.id()]);
    let tx1 = generate_executed_tx_with_authenticated_notes(&chain, account1.id(), &[note1.id()]);

    chain.add_pending_executed_transaction(&tx0);
    chain.add_pending_executed_transaction(&tx1);
    let blockx = chain.prove_next_block();

    // Build a block with empty inputs whose account tree and nullifier tree root are not the empty
    // roots.
    // If they are the empty roots, we do not run the branches of code that handle empty blocks.
    // --------------------------------------------------------------------------------------------

    let latest_block_header = chain.latest_block_header();
    assert_eq!(latest_block_header.commitment(), blockx.commitment());

    // Sanity check: The account and nullifier tree roots should not be the empty tree roots.
    assert_ne!(latest_block_header.account_root(), AccountTree::new().root());
    assert_ne!(latest_block_header.nullifier_root(), Smt::new().root());

    let (_, empty_partial_blockchain) = chain.latest_selective_partial_blockchain([]);
    assert_eq!(empty_partial_blockchain.block_headers().count(), 0);

    let block_inputs = BlockInputs::new(
        latest_block_header.clone(),
        empty_partial_blockchain.clone(),
        BTreeMap::default(),
        BTreeMap::default(),
        BTreeMap::default(),
    );

    let proposed_block =
        ProposedBlock::new(block_inputs, Vec::new()).context("failed to propose block")?;

    let proven_block = LocalBlockProver::new(MIN_PROOF_SECURITY_LEVEL)
        .prove_without_batch_verification(proposed_block)
        .context("failed to prove proposed block")?;

    // Nothing should be created or updated.
    assert_eq!(proven_block.updated_accounts().len(), 0);
    assert_eq!(proven_block.output_note_batches().len(), 0);
    assert_eq!(proven_block.created_nullifiers().len(), 0);
    assert!(proven_block.build_output_note_tree().is_empty());

    // Account and nullifier root should match the previous block header's roots, since nothing has
    // changed.
    assert_eq!(proven_block.header().account_root(), latest_block_header.account_root());
    assert_eq!(proven_block.header().nullifier_root(), latest_block_header.nullifier_root());
    // Block note tree should be the empty root.
    assert_eq!(proven_block.header().note_root(), BlockNoteTree::empty().root());

    // The previous block header should have been added to the chain.
    assert_eq!(
        proven_block.header().chain_commitment(),
        chain.blockchain().peaks().hash_peaks()
    );
    assert_eq!(proven_block.header().block_num(), latest_block_header.block_num() + 1);

    Ok(())
}
