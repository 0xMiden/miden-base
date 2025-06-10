pub mod bench_functions;
pub mod utils;

pub mod benchmark_names {
    pub const BENCH_CONSUME_NOTE_NEW_ACCOUNT: &str = "prove_consume_note_with_new_account";
    pub const BENCH_CONSUME_MULTIPLE_NOTES: &str = "prove_consume_multiple_notes";
    pub const BENCH_GROUP: &str = "miden_proving";
}
