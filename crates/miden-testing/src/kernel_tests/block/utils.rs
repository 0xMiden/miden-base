use std::{collections::BTreeMap, vec, vec::Vec};

use miden_lib::{note::create_p2id_note, transaction::TransactionKernel, utils::ScriptBuilder};
use miden_objects::{
    Felt, ONE, Word, ZERO,
    account::{Account, AccountId},
    asset::{Asset, FungibleAsset},
    batch::ProvenBatch,
    block::BlockNumber,
    crypto::rand::RpoRandomCoin,
    note::{Note, NoteId, NoteTag, NoteType},
    testing::{account_id::ACCOUNT_ID_SENDER, note::NoteBuilder},
    transaction::{ExecutedTransaction, OutputNote, ProvenTransaction, TransactionScript},
};
use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::{Auth, MockChain, TxContextInput, mock_chain::ProvenTransactionExt};

pub struct TestSetup {
    pub chain: MockChain,
    pub accounts: BTreeMap<usize, Account>,
    pub txs: BTreeMap<usize, ProvenTransaction>,
}

pub fn generate_tracked_note(
    chain: &mut MockChain,
    sender: AccountId,
    receiver: AccountId,
) -> Note {
    let note = generate_untracked_note_internal(sender, receiver, vec![]);
    chain.add_pending_note(OutputNote::Full(note.clone()));
    note
}

pub fn generate_tracked_note_with_asset(
    chain: &mut MockChain,
    sender: AccountId,
    receiver: AccountId,
    asset: Asset,
) -> Note {
    let note = generate_untracked_note_internal(sender, receiver, vec![asset]);
    chain.add_pending_note(OutputNote::Full(note.clone()));
    note
}

pub fn generate_untracked_note(sender: AccountId, receiver: AccountId) -> Note {
    generate_untracked_note_internal(sender, receiver, vec![])
}

/// Creates a NOP output note sent by the given sender.
pub fn generate_output_note(sender: AccountId, seed: [u8; 32]) -> Note {
    let mut rng = SmallRng::from_seed(seed);
    NoteBuilder::new(sender, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::for_local_use_case(0, 0).unwrap().into())
        .build(&TransactionKernel::assembler().with_debug_mode(true))
        .unwrap()
}

fn generate_untracked_note_internal(
    sender: AccountId,
    receiver: AccountId,
    asset: Vec<Asset>,
) -> Note {
    // Use OS-randomness so that notes with the same sender and target have different note IDs.
    let mut rng = RpoRandomCoin::new(Word::new([
        Felt::new(rand::rng().random()),
        Felt::new(rand::rng().random()),
        Felt::new(rand::rng().random()),
        Felt::new(rand::rng().random()),
    ]));
    create_p2id_note(sender, receiver, asset, NoteType::Public, Default::default(), &mut rng)
        .unwrap()
}

pub fn generate_fungible_asset(amount: u64, faucet_id: AccountId) -> Asset {
    FungibleAsset::new(faucet_id, amount).unwrap().into()
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
    tx_context.execute().unwrap()
}

pub fn generate_tx_with_authenticated_notes(
    chain: &mut MockChain,
    account_id: AccountId,
    notes: &[NoteId],
) -> ProvenTransaction {
    let executed_tx = generate_executed_tx_with_authenticated_notes(chain, account_id, notes);
    ProvenTransaction::from_executed_transaction_mocked(executed_tx)
}

/// Generates a transaction, which depending on the `modify_storage` flag, does the following:
/// - if `modify_storage` is true, it increments the storage item of the account.
/// - if `modify_storage` is false, it does nothing (NOOP).
///
/// To make this transaction (always) non-empty, it consumes one "noop note", which does nothing.
pub fn generate_conditional_tx(
    chain: &mut MockChain,
    input: impl Into<TxContextInput>,
    modify_storage: bool,
) -> ExecutedTransaction {
    let noop_note = NoteBuilder::new(ACCOUNT_ID_SENDER.try_into().unwrap(), &mut rand::rng())
        .build(&TransactionKernel::assembler())
        .expect("failed to create the noop note");
    chain.add_pending_note(OutputNote::Full(noop_note.clone()));
    chain.prove_next_block().unwrap();

    let auth_args = [
        if modify_storage { ONE } else { ZERO }, // increment nonce if modify_storage is true
        Felt::new(99),
        Felt::new(98),
        Felt::new(97),
    ];

    let tx_context = chain
        .build_tx_context(input.into(), &[noop_note.id()], &[])
        .unwrap()
        .extend_input_notes(vec![noop_note])
        .auth_args(auth_args.into())
        .build()
        .unwrap();
    tx_context.execute().unwrap()
}

/// Generates a transaction that expires at the given block number.
pub fn generate_tx_with_expiration(
    chain: &mut MockChain,
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
    let executed_tx = tx_context.execute().unwrap();
    ProvenTransaction::from_executed_transaction_mocked(executed_tx)
}

pub fn generate_tx_with_unauthenticated_notes(
    chain: &mut MockChain,
    account_id: AccountId,
    notes: &[Note],
) -> ProvenTransaction {
    let tx_context = chain
        .build_tx_context(account_id, &[], notes)
        .expect("failed to build tx context")
        .build()
        .unwrap();
    let executed_tx = tx_context.execute().unwrap();
    ProvenTransaction::from_executed_transaction_mocked(executed_tx)
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

pub fn generate_batch(chain: &mut MockChain, txs: Vec<ProvenTransaction>) -> ProvenBatch {
    chain
        .propose_transaction_batch(txs)
        .map(|batch| chain.prove_transaction_batch(batch).unwrap())
        .unwrap()
}

/// Setup a test mock chain with the number of accounts, notes and transactions.
///
/// This is merely generating some valid data for testing purposes.
pub fn setup_chain(num_accounts: usize) -> TestSetup {
    let mut builder = MockChain::builder();
    let sender_account = builder
        .add_existing_mock_account(Auth::IncrNonce)
        .expect("adding account should be valid");
    let mut accounts = BTreeMap::new();
    let mut notes = BTreeMap::new();
    let mut txs = BTreeMap::new();

    for i in 0..num_accounts {
        let account = builder
            .add_existing_mock_account(Auth::IncrNonce)
            .expect("adding account should be valid");
        let note = builder
            .add_p2id_note(sender_account.id(), account.id(), &[], NoteType::Public)
            .expect("adding p2id note should be valid");
        accounts.insert(i, account);
        notes.insert(i, note);
    }

    let mut chain = builder.build().expect("building chain should be valid");

    chain.prove_next_block().expect("failed to prove block");

    for i in 0..num_accounts {
        let tx =
            generate_tx_with_authenticated_notes(&mut chain, accounts[&i].id(), &[notes[&i].id()]);
        txs.insert(i, tx);
    }

    TestSetup { chain, accounts, txs }
}
