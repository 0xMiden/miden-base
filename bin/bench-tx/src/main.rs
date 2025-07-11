use core::fmt;
use std::{fs::File, io::Write, path::Path};

use anyhow::Context;
use miden_lib::{note::create_p2id_note, transaction::TransactionKernel};
use miden_objects::{
    Felt, FieldElement, Word,
    account::{Account, AccountId, AccountStorageMode, AccountType},
    asset::{Asset, FungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::NoteType,
    testing::{
        account_component::IncrNonceAuthComponent,
        account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    },
    transaction::{TransactionMeasurements, TransactionScript},
};
use miden_testing::{TransactionContextBuilder, utils::create_p2any_note};

mod utils;
use utils::{
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_SENDER, DEFAULT_AUTH_SCRIPT,
    get_account_with_basic_authenticated_wallet, get_new_pk_and_authenticator,
    write_bench_results_to_json,
};
pub enum Benchmark {
    Simple,
    P2ID,
}

impl fmt::Display for Benchmark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Benchmark::Simple => write!(f, "simple"),
            Benchmark::P2ID => write!(f, "p2id"),
        }
    }
}

fn main() -> anyhow::Result<()> {
    // create a template file for benchmark results
    let path = Path::new("bin/bench-tx/bench-tx.json");
    let mut file = File::create(path).context("failed to create file")?;
    file.write_all(b"{}").context("failed to write to file")?;

    // run all available benchmarks
    let benchmark_results = vec![
        (Benchmark::Simple, benchmark_default_tx()?.into()),
        (Benchmark::P2ID, benchmark_p2id()?.into()),
    ];

    // store benchmark results in the JSON file
    write_bench_results_to_json(path, benchmark_results)?;

    Ok(())
}

// BENCHMARKS
// ================================================================================================

/// Runs the default transaction with empty transaction script and two default notes.
#[allow(clippy::arc_with_non_send_sync)]
pub fn benchmark_default_tx() -> anyhow::Result<TransactionMeasurements> {
    let assembler = TransactionKernel::testing_assembler();
    let auth_component = IncrNonceAuthComponent::new(assembler.clone()).unwrap();

    let tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            auth_component,
            assembler,
        );

        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);

        let input_note_2 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(150)]);
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2])
            .build()?
    };
    let executed_transaction = tx_context.execute().context("failed to execute transaction")?;

    Ok(executed_transaction.into())
}

/// Runs the transaction which consumes a P2ID note into a basic wallet.
#[allow(clippy::arc_with_non_send_sync)]
pub fn benchmark_p2id() -> anyhow::Result<TransactionMeasurements> {
    // Create assets
    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
    let fungible_asset: Asset = FungibleAsset::new(faucet_id, 100).unwrap().into();

    // Create sender and target account
    let sender_account_id = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let (target_pub_key, falcon_auth) = get_new_pk_and_authenticator();

    let target_account = get_account_with_basic_authenticated_wallet(
        [10; 32],
        AccountType::RegularAccountUpdatableCode,
        AccountStorageMode::Private,
        target_pub_key,
        None,
    );

    // Create the note
    let note = create_p2id_note(
        sender_account_id,
        target_account.id(),
        vec![fungible_asset],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    let tx_script_target =
        TransactionScript::compile(DEFAULT_AUTH_SCRIPT, TransactionKernel::assembler()).unwrap();

    let tx_context = TransactionContextBuilder::new(target_account.clone())
        .extend_input_notes(vec![note])
        .tx_script(tx_script_target)
        .authenticator(Some(falcon_auth))
        .build()?;

    let executed_transaction = tx_context.execute()?;

    Ok(executed_transaction.into())
}
