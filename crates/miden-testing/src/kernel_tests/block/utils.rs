use std::{collections::BTreeMap, vec, vec::Vec};

use miden_lib::{note::create_p2id_note, transaction::TransactionKernel};
use miden_objects::{
    Felt, Word,
    account::{Account, AccountId, AccountStorageMode},
    asset::{Asset, FungibleAsset},
    batch::ProvenBatch,
    block::BlockNumber,
    crypto::rand::RpoRandomCoin,
    note::{Note, NoteId, NoteTag, NoteType},
    testing::{account_component::AccountMockComponent, note::NoteBuilder},
    transaction::{ExecutedTransaction, OutputNote, ProvenTransaction, TransactionScript},
};
use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::{AccountState, Auth, MockChain, TxContextInput, mock_chain::ProvenTransactionExt};

pub struct TestSetup {
    pub chain: MockChain,
    pub accounts: BTreeMap<usize, Account>,
    pub txs: BTreeMap<usize, ProvenTransaction>,
}

pub fn generate_account(chain: &mut MockChain) -> Account {
    let account_builder = Account::builder(rand::rng().random())
        .storage_mode(AccountStorageMode::Public)
        .with_component(
            AccountMockComponent::new_with_empty_slots(TransactionKernel::assembler()).unwrap(),
        );
    chain
        .add_pending_account_from_builder(Auth::IncrNonce, account_builder, AccountState::Exists)
        .expect("failed to add pending account from builder")
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

/// Creates an NOP output note sent by the given sender.
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

    TransactionScript::compile(code, TransactionKernel::testing_assembler_with_mock_account())
        .unwrap()
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
    let mut chain = MockChain::new();
    let sender_account = generate_account(&mut chain);
    let mut accounts = BTreeMap::new();
    let mut notes = BTreeMap::new();
    let mut txs = BTreeMap::new();

    for i in 0..num_accounts {
        let account = generate_account(&mut chain);
        let note = generate_tracked_note(&mut chain, sender_account.id(), account.id());
        accounts.insert(i, account);
        notes.insert(i, note);
    }

    chain.prove_next_block().expect("failed to prove block");

    for i in 0..num_accounts {
        let tx =
            generate_tx_with_authenticated_notes(&mut chain, accounts[&i].id(), &[notes[&i].id()]);
        txs.insert(i, tx);
    }

    TestSetup { chain, accounts, txs }
}
