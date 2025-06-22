use super::{AdviceInputs, TransactionAdvice, TransactionInputs};
use crate::utils::serde::{ByteReader, Deserializable, DeserializationError, Serializable};

// TRANSACTION WITNESS
// ================================================================================================

/// Transaction witness contains all the data required to execute and prove a Miden blockchain
/// transaction.
///
/// The main purpose of the transaction witness is to enable stateless re-execution and proving
/// of transactions.
///
/// A transaction witness consists of:
/// - Transaction inputs which contain information about the initial state of the account, input
///   notes, block header etc.
/// - Optional transaction advice arguments which may contain a transaction script, note arguments,
///   transaction script argument and any additional advice data to initialize the advice provider
///   with prior to transaction execution.
/// - Advice witness which contains all data requested by the VM from the advice provider while
///   executing the transaction program.
///
/// TODO: currently, the advice witness contains redundant and irrelevant data (e.g., tx inputs
/// and tx outputs; account codes and a subset of that data in advice inputs).
/// We should optimize it to contain only the minimum data required for executing/proving the
/// transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionWitness {
    pub tx_inputs: TransactionInputs,
    pub tx_advice: TransactionAdvice,
    pub advice_witness: AdviceInputs,
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionWitness {
    fn write_into<W: miden_crypto::utils::ByteWriter>(&self, target: &mut W) {
        self.tx_inputs.write_into(target);
        self.tx_advice.write_into(target);
        self.advice_witness.write_into(target);
    }
}

impl Deserializable for TransactionWitness {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tx_inputs = TransactionInputs::read_from(source)?;
        let tx_advice = TransactionAdvice::read_from(source)?;
        let advice_witness = AdviceInputs::read_from(source)?;
        Ok(Self { tx_inputs, tx_advice, advice_witness })
    }
}
