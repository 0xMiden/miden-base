use std::vec::Vec;

use miden_lib::testing::note::NoteBuilder;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::AccountId;
use miden_objects::batch::ProvenBatch;
use miden_objects::block::BlockNumber;
use miden_objects::note::{Note, NoteId, NoteTag, NoteType};
use miden_objects::transaction::{ExecutedTransaction, ProvenTransaction, TransactionScript};
use miden_objects::{Felt, ONE, ZERO};
use miden_tx::LocalTransactionProver;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use crate::{MockChain, TxContextInput};

/// Creates a NOP output note sent by the given sender.
pub fn generate_output_note(sender: AccountId, seed: [u8; 32]) -> Note {
    let mut rng = SmallRng::from_seed(seed);
    NoteBuilder::new(sender, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::for_local_use_case(0, 0).unwrap().into())
        .build()
        .unwrap()
}

pub fn generate_executed_tx_with_authenticated_notes(
    chain: &MockChain,
    input: impl Into<TxContextInput>,
    notes: &[NoteId],
) -> ExecutedTransaction {
    let tx_context = chain
        .build_tx_context(input, notes, &[])
        .expect("failed to build tx context")
        .build()
        .unwrap();
    tx_context.execute_blocking().unwrap()
}

pub fn generate_tx_with_authenticated_notes(
    chain: &MockChain,
    input: impl Into<TxContextInput>,
    notes: &[NoteId],
) -> ProvenTransaction {
    let executed_tx = generate_executed_tx_with_authenticated_notes(chain, input, notes);
    LocalTransactionProver::default().prove_dummy(executed_tx).unwrap()
}

/// Generates a transaction, which depending on the `modify_storage` flag, does the following:
/// - if `modify_storage` is true, it increments the storage item of the account.
/// - if `modify_storage` is false, it does nothing (NOOP).
///
/// To make this transaction (always) non-empty, it consumes one "noop note", which does nothing.
pub fn generate_conditional_tx(
    builder: &mut MockChain,
    input: impl Into<TxContextInput>,
    noop_note: Note,
    modify_storage: bool,
) -> ExecutedTransaction {
    let auth_args = [
        if modify_storage { ONE } else { ZERO }, // increment nonce if modify_storage is true
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
    ];

    let tx_context = builder
        .build_tx_context(input.into(), &[noop_note.id()], &[])
        .unwrap()
        .auth_args(auth_args.into())
        .build()
        .unwrap();
    tx_context.execute_blocking().unwrap()
}

/// Generates a transaction that expires at the given block number.
pub fn generate_tx_with_expiration(
    chain: &MockChain,
    input: impl Into<TxContextInput>,
    expiration_block: BlockNumber,
) -> ProvenTransaction {
    let expiration_delta = expiration_block
        .checked_sub(chain.latest_block_header().block_num().as_u32())
        .unwrap();

    let tx_context = chain
        .build_tx_context(input, &[], &[])
        .expect("failed to build tx context")
        .tx_script(update_expiration_tx_script(expiration_delta.as_u32() as u16))
        .build()
        .unwrap();
    let executed_tx = tx_context.execute_blocking().unwrap();
    LocalTransactionProver::default().prove_dummy(executed_tx).unwrap()
}

pub fn generate_tx_with_unauthenticated_notes(
    chain: &MockChain,
    account_id: AccountId,
    notes: &[Note],
) -> ProvenTransaction {
    let tx_context = chain
        .build_tx_context(account_id, &[], notes)
        .expect("failed to build tx context")
        .build()
        .unwrap();
    let executed_tx = tx_context.execute_blocking().unwrap();
    LocalTransactionProver::default().prove_dummy(executed_tx).unwrap()
}

fn update_expiration_tx_script(expiration_delta: u16) -> TransactionScript {
    let code = format!(
        "
        use.miden::tx

        begin
            push.{expiration_delta}
            exec.tx::update_expiration_block_delta
        end
        "
    );

    ScriptBuilder::default().compile_tx_script(code).unwrap()
}

pub fn generate_batch(chain: &MockChain, txs: Vec<ProvenTransaction>) -> ProvenBatch {
    chain
        .propose_transaction_batch(txs)
        .map(|batch| chain.prove_transaction_batch(batch).unwrap())
        .unwrap()
}

/// TODO
pub trait MockChainBuilderBlockExt {
    fn generate_executed_tx_with_authenticated_notes(
        &self,
        input: impl Into<TxContextInput>,
        notes: impl IntoIterator<Item = NoteId>,
    ) -> ExecutedTransaction;

    fn create_authenticated_notes_tx(
        &self,
        input: impl Into<TxContextInput>,
        notes: impl IntoIterator<Item = NoteId>,
    ) -> ProvenTransaction;

    fn generate_tx_with_unauthenticated_notes(
        &self,
        account_id: AccountId,
        notes: &[Note],
    ) -> ProvenTransaction;

    fn create_expiring_tx(
        &self,
        input: impl Into<TxContextInput>,
        expiration_block: BlockNumber,
    ) -> ProvenTransaction;

    fn create_batch(&self, txs: Vec<ProvenTransaction>) -> ProvenBatch;
}

impl MockChainBuilderBlockExt for MockChain {
    fn generate_executed_tx_with_authenticated_notes(
        &self,
        input: impl Into<TxContextInput>,
        notes: impl IntoIterator<Item = NoteId>,
    ) -> ExecutedTransaction {
        let notes = notes.into_iter().collect::<Vec<_>>();
        generate_executed_tx_with_authenticated_notes(self, input, &notes)
    }

    fn create_authenticated_notes_tx(
        &self,
        input: impl Into<TxContextInput>,
        notes: impl IntoIterator<Item = NoteId>,
    ) -> ProvenTransaction {
        let notes = notes.into_iter().collect::<Vec<_>>();
        generate_tx_with_authenticated_notes(self, input, &notes)
    }

    fn generate_tx_with_unauthenticated_notes(
        &self,
        account_id: AccountId,
        notes: &[Note],
    ) -> ProvenTransaction {
        generate_tx_with_unauthenticated_notes(self, account_id, notes)
    }

    fn create_expiring_tx(
        &self,
        input: impl Into<TxContextInput>,
        expiration_block: BlockNumber,
    ) -> ProvenTransaction {
        generate_tx_with_expiration(self, input, expiration_block)
    }

    fn create_batch(&self, txs: Vec<ProvenTransaction>) -> ProvenBatch {
        generate_batch(self, txs)
    }
}
