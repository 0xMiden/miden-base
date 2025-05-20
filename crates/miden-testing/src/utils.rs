use miden_lib::transaction::{memory, TransactionKernel};
use miden_objects::{
    account::AccountId,
    asset::FungibleAsset,
    crypto::utils::Serializable,
    note::{Note, NoteAssets, NoteInputs, NoteMetadata, NoteRecipient, NoteScript, NoteType},
    testing::account_id::ACCOUNT_ID_SENDER,
    transaction::{ExecutedTransaction, ProvenTransaction},
    Felt, Word, ZERO,
};
use miden_tx::{
    LocalTransactionProver, ProvingOptions, TransactionProver, TransactionVerifier,
    TransactionVerifierError,
};
use vm_processor::utils::Deserializable;

// TEST HELPERS
// ================================================================================================

pub fn input_note_data_ptr(note_idx: u32) -> memory::MemoryAddress {
    memory::INPUT_NOTE_DATA_SECTION_OFFSET + note_idx * memory::NOTE_MEM_SIZE
}

pub fn prove_and_verify_transaction(
    executed_transaction: ExecutedTransaction,
) -> Result<(), TransactionVerifierError> {
    let executed_transaction_id = executed_transaction.id();
    // Prove the transaction

    let proof_options = ProvingOptions::default();
    let prover = LocalTransactionProver::new(proof_options);
    let proven_transaction = prover.prove(executed_transaction.into()).unwrap();

    assert_eq!(proven_transaction.id(), executed_transaction_id);

    // Serialize & deserialize the ProvenTransaction
    let serialised_transaction = proven_transaction.to_bytes();
    let proven_transaction = ProvenTransaction::read_from_bytes(&serialised_transaction).unwrap();

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
    const SERIAL_NUM: Word = [Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)];
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let vault = NoteAssets::new(vec![fungible_asset.into()]).unwrap();
    let metadata =
        NoteMetadata::new(sender_id, NoteType::Public, 1.into(), NoteExecutionHint::Always, ZERO)
            .unwrap();
    let inputs = NoteInputs::new(vec![]).unwrap();
    let recipient = NoteRecipient::new(SERIAL_NUM, note_script, inputs);

    Note::new(vault, metadata, recipient)
}
