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

        #[cfg(any(feature = "testing", test))]
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

#[cfg(test)]
mod tests {

    use alloc::vec::Vec;

    use assembly::Assembler;
    use miden_crypto::{Felt, Word};
    use vm_core::ZERO;
    use vm_processor::AdviceInputs;

    use crate::account::{
        Account,
        AccountCode,
        AccountDelta,
        AccountHeader,
        AccountId,
        AccountIdVersion,
        AccountStorage,
        AccountStorageDelta,
        AccountStorageMode,
        AccountType,
        AccountVaultDelta,
        StorageSlot,
    };
    use crate::asset::{Asset, AssetVault, FungibleAsset};
    use crate::block::{BlockHeader, BlockNumber};
    use crate::testing::account_id::{
        ACCOUNT_ID_PRIVATE_SENDER,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    };
    use crate::testing::note::NoteBuilder;
    use crate::transaction::{
        InputNotes,
        OutputNote,
        OutputNotes,
        PartialBlockchain,
        TransactionArgs,
        TransactionInputs,
        TransactionOutputs,
        TransactionWitness,
    };

    pub fn build_account(assets: Vec<Asset>, nonce: Felt, slots: Vec<StorageSlot>) -> Account {
        let id = AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap();
        let code = AccountCode::mock();

        let vault = AssetVault::new(&assets).unwrap();

        let storage = AccountStorage::new(slots).unwrap();

        Account::from_parts(id, vault, storage, code, nonce)
    }

    #[test]
    fn transaction_witness_serialization_roundtrip() {
        use crate::utils::serde::{Deserializable, Serializable};

        let partial_blockchain = PartialBlockchain::default();
        let account = {
            let asset_0 = FungibleAsset::mock(10);
            let init_nonce = Felt::new(1);
            let word = Word::default();
            let storage_slot = StorageSlot::Value(word);
            build_account(vec![asset_0], init_nonce, vec![storage_slot])
        };

        let block_header = {
            let chain_commitment = partial_blockchain.peaks().hash_peaks();
            let note_root = Word::default();
            let tx_kernel_commitment = Word::default();
            BlockHeader::mock(0, Some(chain_commitment), Some(note_root), &[], tx_kernel_commitment)
        };

        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        let account_delta = {
            let storage_delta = AccountStorageDelta::new();
            let vault_delta = AccountVaultDelta::default();

            AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), ZERO).unwrap()
        };

        let tx_inputs = TransactionInputs::new(
            account,
            None,
            block_header.clone(),
            partial_blockchain.clone(),
            InputNotes::default(),
        )
        .unwrap();

        let tx_outputs = {
            let account_header = AccountHeader::new(
                account_id,
                Felt::default(),
                Word::default(),
                Word::default(),
                Word::default(),
            );

            let mock_note = NoteBuilder::new(account_id, &mut rand::rng())
                .build(&Assembler::default())
                .unwrap();

            let mut fungible_faucet_id_bytes = [0; 15];
            fungible_faucet_id_bytes[0] = 0xcd;
            fungible_faucet_id_bytes[1] = 0xb1;

            let mut non_fungible_faucet_id_bytes = [0; 15];
            non_fungible_faucet_id_bytes[0] = 0xab;
            non_fungible_faucet_id_bytes[1] = 0xec;

            let offered_asset = FungibleAsset::new(
                AccountId::dummy(
                    fungible_faucet_id_bytes,
                    AccountIdVersion::Version0,
                    AccountType::FungibleFaucet,
                    AccountStorageMode::Public,
                ),
                2500,
            )
            .unwrap();

            TransactionOutputs {
                account: account_header,
                account_delta_commitment: Word::default(),
                output_notes: OutputNotes::new(vec![OutputNote::Full(mock_note)]).unwrap(),
                fee: offered_asset,
                expiration_block_num: BlockNumber::default(),
            }
        };

        let witness = TransactionWitness {
            tx_inputs,
            tx_args: TransactionArgs::default(),
            advice_witness: AdviceInputs::default(),
            account_delta,
            tx_outputs,
        };

        let mut bytes = Vec::new();
        witness.write_into(&mut bytes);

        let deserialized = TransactionWitness::read_from_bytes(&bytes).unwrap();

        assert_eq!(witness, deserialized);
    }
}
