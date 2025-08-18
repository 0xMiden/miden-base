#[cfg(any(feature = "testing", test))]
use super::TransactionOutputs;
use super::{AdviceInputs, TransactionArgs, TransactionInputs};
#[cfg(any(feature = "testing", test))]
use crate::account::AccountDelta;
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
/// - Optional transaction arguments which may contain a transaction script, note arguments,
///   transaction script arguments and any additional advice data to initialize the advice provider
///   with prior to transaction execution.
/// - Advice witness which contains all data requested by the VM from the advice provider while
///   executing the transaction program.
///
/// TODO: currently, the advice witness contains redundant and irrelevant data (e.g., tx inputs
/// and tx outputs; account codes and a subset of that data in advice inputs).
/// We should optimize it to contain only the minimum data required for executing/proving the
/// transaction.
#[cfg(not(any(feature = "testing", test)))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionWitness {
    pub tx_inputs: TransactionInputs,
    pub tx_args: TransactionArgs,
    pub advice_witness: AdviceInputs,
}

/// Please see the docs for the non-testing variant.
#[cfg(any(feature = "testing", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionWitness {
    pub tx_inputs: TransactionInputs,
    pub tx_args: TransactionArgs,
    pub advice_witness: AdviceInputs,
    pub account_delta: AccountDelta,
    pub tx_outputs: TransactionOutputs,
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionWitness {
    fn write_into<W: miden_crypto::utils::ByteWriter>(&self, target: &mut W) {
        self.tx_inputs.write_into(target);
        self.tx_args.write_into(target);
        self.advice_witness.write_into(target);

        #[cfg(not(any(feature = "testing", test)))]
        {
            self.account_delta.write_into(target);
            self.tx_outputs.write_into(target);
        }
    }
}

impl Deserializable for TransactionWitness {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let tx_inputs = TransactionInputs::read_from(source)?;
        let tx_args = TransactionArgs::read_from(source)?;
        let advice_witness = AdviceInputs::read_from(source)?;

        #[cfg(not(any(feature = "testing", test)))]
        {
            Ok(Self { tx_inputs, tx_args, advice_witness })
        }

        #[cfg(any(feature = "testing", test))]
        {
            Ok(Self {
                tx_inputs,
                tx_args,
                advice_witness,
                account_delta: AccountDelta::read_from(source)?,
                tx_outputs: TransactionOutputs::read_from(source)?,
            })
        }
    }
}
