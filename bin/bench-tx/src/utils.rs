extern crate alloc;
pub use alloc::{collections::BTreeMap, string::String};
use std::fs::{read_to_string, write};

use anyhow::Context;
use miden_lib::account::{auth::AuthRpoFalcon512, wallets::BasicWallet};
use miden_objects::{
    account::{Account, AccountBuilder, AccountStorageMode, AccountType, AuthSecretKey},
    asset::Asset,
    crypto::dsa::rpo_falcon512::{PublicKey, SecretKey},
    transaction::TransactionMeasurements,
};
use miden_tx::auth::BasicAuthenticator;
use rand_chacha::{ChaCha20Rng, rand_core::SeedableRng};
use serde::Serialize;
use serde_json::{Value, from_str, to_string_pretty};

use super::{Benchmark, Path};

// CONSTANTS
// ================================================================================================

// Copied from miden_objects::testing::account_id.
pub const ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET: u128 = 0x00aa00000000bc200000bc000000de00;
pub const ACCOUNT_ID_SENDER: u128 = 0x00fa00000000bb800000cc000000de00;

// MEASUREMENTS PRINTER
// ================================================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MeasurementsPrinter {
    prologue: usize,
    notes_processing: usize,
    note_execution: BTreeMap<String, usize>,
    tx_script_processing: usize,
    epilogue: usize,
    after_compute_fee_cycles: usize,
}

impl From<TransactionMeasurements> for MeasurementsPrinter {
    fn from(tx_measurements: TransactionMeasurements) -> Self {
        let note_execution_map = tx_measurements
            .note_execution
            .iter()
            .map(|(id, len)| (id.to_hex(), *len))
            .collect();

        MeasurementsPrinter {
            prologue: tx_measurements.prologue,
            notes_processing: tx_measurements.notes_processing,
            note_execution: note_execution_map,
            tx_script_processing: tx_measurements.tx_script_processing,
            epilogue: tx_measurements.epilogue,
            after_compute_fee_cycles: tx_measurements.after_compute_fee_cycles,
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

pub fn get_account_with_basic_authenticated_wallet(
    init_seed: [u8; 32],
    account_type: AccountType,
    storage_mode: AccountStorageMode,
    public_key: PublicKey,
    assets: Option<Asset>,
) -> Account {
    AccountBuilder::new(init_seed)
        .account_type(account_type)
        .storage_mode(storage_mode)
        .with_assets(assets)
        .with_component(BasicWallet)
        .with_auth_component(AuthRpoFalcon512::new(public_key))
        .build_existing()
        .unwrap()
}

pub fn get_new_pk_and_authenticator() -> (PublicKey, BasicAuthenticator<ChaCha20Rng>) {
    let mut rng = ChaCha20Rng::from_seed(Default::default());
    let sec_key = SecretKey::with_rng(&mut rng);
    let pub_key = sec_key.public_key();

    let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key.into(), AuthSecretKey::RpoFalcon512(sec_key))],
        rng,
    );

    (pub_key, authenticator)
}

pub fn write_bench_results_to_json(
    path: &Path,
    tx_benchmarks: Vec<(Benchmark, MeasurementsPrinter)>,
) -> anyhow::Result<()> {
    // convert benchmark file internals to the JSON Value
    let benchmark_file = read_to_string(path).context("failed to read benchmark file")?;
    let mut benchmark_json: Value =
        from_str(&benchmark_file).context("failed to convert benchmark contents to json")?;

    // fill benchmarks JSON with results of each benchmark
    for (bench_type, tx_progress) in tx_benchmarks {
        let tx_benchmark_json = serde_json::to_value(tx_progress)
            .context("failed to convert tx measurements to json")?;

        benchmark_json[bench_type.to_string()] = tx_benchmark_json;
    }

    // write the benchmarks JSON to the results file
    write(
        path,
        to_string_pretty(&benchmark_json).expect("failed to convert json to String"),
    )
    .context("failed to write benchmark results to file")?;

    Ok(())
}
