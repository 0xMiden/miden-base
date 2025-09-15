use core::fmt;

pub mod utils;

/// Indicates the type of the transaction execution benchmark
pub enum ExecutionBenchmark {
    ConsumeSingleP2ID,
    ConsumeMultipleP2ID,
    CreateSingleP2ID,
}

impl fmt::Display for ExecutionBenchmark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionBenchmark::ConsumeSingleP2ID => write!(f, "consume single P2ID note"),
            ExecutionBenchmark::ConsumeMultipleP2ID => write!(f, "consume multiple P2ID notes"),
            ExecutionBenchmark::CreateSingleP2ID => write!(f, "create single P2ID note"),
        }
    }
}
