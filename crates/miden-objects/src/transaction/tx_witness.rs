use alloc::vec::Vec;

use super::AdviceInputs;
use crate::account::{AccountCode, PartialAccount};
use crate::block::BlockHeader;
use crate::transaction::{
    InputNote,
    InputNotes,
    PartialBlockchain,
    TransactionArgs,
    TransactionPreparationInputs,
};
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
/// - Account code needed for invoking procedures on foreign accounts.
/// - Advice witness which contains all data that is in the advice provider by the end of the
///   transaction execution.
///
/// TODO: currently, the advice witness contains redundant and irrelevant data (e.g., tx inputs
/// and tx outputs; account codes and a subset of that data in advice inputs).
/// We should optimize it to contain only the minimum data required for executing/proving the
/// transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionWitness {
    pub prep_inputs: TransactionPreparationInputs,
    pub input_notes: InputNotes<InputNote>,
    pub tx_args: TransactionArgs,
    pub foreign_account_code: Vec<AccountCode>,
    pub advice_witness: AdviceInputs,
}

// SERIALIZATION
// ================================================================================================

impl Serializable for TransactionWitness {
    fn write_into<W: miden_crypto::utils::ByteWriter>(&self, target: &mut W) {
        self.prep_inputs.account().write_into(target);
        self.prep_inputs.block_header().write_into(target);
        self.prep_inputs.blockchain().write_into(target);
        self.input_notes.write_into(target);
        self.tx_args.write_into(target);
        self.foreign_account_code.write_into(target);
        self.advice_witness.write_into(target);
    }
}

impl Deserializable for TransactionWitness {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let account = PartialAccount::read_from(source)?;
        let block_header = BlockHeader::read_from(source)?;
        let blockchain = PartialBlockchain::read_from(source)?;
        let input_notes = InputNotes::<InputNote>::read_from(source)?;
        let tx_args = TransactionArgs::read_from(source)?;
        let foreign_account_code = <Vec<AccountCode>>::read_from(source)?;
        let advice_witness = AdviceInputs::read_from(source)?;

        let prep_inputs = TransactionPreparationInputs::new(account, block_header, blockchain)
            .map_err(|err| DeserializationError::InvalidValue(format!("{err}")))?;
        Ok(Self {
            prep_inputs,
            input_notes,
            tx_args,
            foreign_account_code,
            advice_witness,
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use miden_crypto::Word;

    use crate::account::{AccountBuilder, AccountComponent, StorageSlot};
    use crate::assembly::Assembler;
    use crate::asset::FungibleAsset;
    use crate::block::{BlockHeader, BlockNumber};
    use crate::testing::noop_auth_component::NoopAuthComponent;
    use crate::transaction::{
        InputNotes,
        PartialBlockchain,
        TransactionArgs,
        TransactionPreparationInputs,
        TransactionWitness,
    };
    use crate::vm::AdviceInputs;

    #[test]
    fn transaction_witness_serialization_roundtrip() -> anyhow::Result<()> {
        use crate::utils::serde::{Deserializable, Serializable};

        let component = AccountComponent::compile(
            "export.foo add.1 end",
            Assembler::default(),
            vec![StorageSlot::Value(Word::empty())],
        )?
        .with_supports_all_types();
        let asset = FungibleAsset::mock(200);
        let account = AccountBuilder::new([1; 32])
            .with_auth_component(NoopAuthComponent)
            .with_component(component)
            .with_assets([asset])
            .build_existing()?;

        let partial_blockchain = PartialBlockchain::default();
        let block_header = BlockHeader::mock(
            BlockNumber::GENESIS,
            Some(partial_blockchain.peaks().hash_peaks()),
            None,
            &[],
            Word::empty(),
        );

        let prep_inputs = TransactionPreparationInputs::new(
            (&account).into(),
            block_header.clone(),
            partial_blockchain.clone(),
        )
        .unwrap();

        let witness = TransactionWitness {
            prep_inputs,
            input_notes: InputNotes::default(),
            tx_args: TransactionArgs::default(),
            foreign_account_code: vec![account.code().clone()],
            advice_witness: AdviceInputs::default(),
        };

        let bytes = witness.to_bytes();
        let deserialized = TransactionWitness::read_from_bytes(&bytes)
            .context("failed to deserialize tx witness")?;

        assert_eq!(witness, deserialized);

        Ok(())
    }
}
