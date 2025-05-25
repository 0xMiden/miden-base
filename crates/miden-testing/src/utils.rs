use miden_lib::transaction::{TransactionKernel, memory};
use miden_objects::{
    Felt, ONE, Word, ZERO,
    account::AccountId,
    asset::FungibleAsset,
    crypto::utils::Serializable,
    note::{Note, NoteAssets, NoteInputs, NoteMetadata, NoteRecipient, NoteScript, NoteType},
    testing::account_id::ACCOUNT_ID_SENDER,
    transaction::{ExecutedTransaction, ProvenTransaction},
    utils::Deserializable,
};
use miden_tx::{
    LocalTransactionProver, ProvingOptions, TransactionProver, TransactionVerifier,
    TransactionVerifierError,
};

// TEST HELPERS
// ================================================================================================

pub fn input_note_data_ptr(note_idx: u32) -> memory::MemoryAddress {
    memory::INPUT_NOTE_DATA_SECTION_OFFSET + note_idx * memory::NOTE_MEM_SIZE
}

pub fn prove_and_verify_transaction(
    executed_transaction: ExecutedTransaction,
) -> Result<(), TransactionVerifierError> {
    // Prove the transaction
    let executed_transaction_id = executed_transaction.id();
    let prover = LocalTransactionProver::new(ProvingOptions::default());
    let proven_transaction = prover.prove(executed_transaction.into()).unwrap();

    assert_eq!(proven_transaction.id(), executed_transaction_id);

    // Serialize & deserialize the ProvenTransaction
    let serialized_transaction = proven_transaction.to_bytes();
    let proven_transaction = ProvenTransaction::read_from_bytes(&serialized_transaction).unwrap();

    // Verify that the generated proof is valid
    let verifier = TransactionVerifier::new(miden_objects::MIN_PROOF_SECURITY_LEVEL);
    verifier.verify(&proven_transaction)
}

pub fn get_note_with_fungible_asset_and_script(
    fungible_asset: FungibleAsset,
    note_script: &str,
) -> Note {
    use miden_objects::note::NoteExecutionHint;

    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let note_script = NoteScript::compile(note_script, assembler).unwrap();
    const SERIAL_NUM: Word = [ONE, Felt::new(2), Felt::new(3), Felt::new(4)];
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let vault = NoteAssets::new(vec![fungible_asset.into()]).unwrap();
    let metadata =
        NoteMetadata::new(sender_id, NoteType::Public, 1.into(), NoteExecutionHint::Always, ZERO)
            .unwrap();
    let inputs = NoteInputs::new(vec![]).unwrap();
    let recipient = NoteRecipient::new(SERIAL_NUM, note_script, inputs);

    Note::new(vault, metadata, recipient)
}
