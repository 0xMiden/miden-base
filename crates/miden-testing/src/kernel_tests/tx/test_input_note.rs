use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    asset::FungibleAsset, note::NoteType, testing::account_id::ACCOUNT_ID_SENDER,
    transaction::TransactionScript,
};

use super::word_to_masm_push_string;
use crate::{MockChain, TxContextInput};

#[test]
fn test_input_note_get_asset_info() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(crate::Auth::BasicAuth)?;
    let p2id_note_0 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(150)],
        NoteType::Public,
    )?;
    let p2id_note_1 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(300)],
        NoteType::Public,
    )?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        r#"
        use.miden::input_note

        begin
            # get the assets hash and assets number from the 0'th input note
            push.0
            exec.input_note::get_assets_info
            # => [ASSETS_COMMITMENT_0, num_assets_0]

            # assert the correctness of the assets hash
            push.{COMPUTED_ASSETS_COMMITMENT_0} 
            assert_eqw.err="note 0 has incorrect assets hash"
            # => [num_assets_0]

            # assert the number of note assets
            push.{assets_number_0}
            assert_eq.err="note 0 has incorrect assets number"
            # => []

            # get the assets hash and assets number from the 1'st input note
            push.1
            exec.input_note::get_assets_info
            # => [ASSETS_COMMITMENT_1, num_assets_1]

            # assert the correctness of the assets hash
            push.{COMPUTED_ASSETS_COMMITMENT_1} 
            assert_eqw.err="note 1 has incorrect assets hash"
            # => [num_assets_1]

            # assert the number of note assets
            push.{assets_number_1}
            assert_eq.err="note 1 has incorrect assets number"
            # => []
        end
    "#,
        COMPUTED_ASSETS_COMMITMENT_0 = word_to_masm_push_string(&p2id_note_0.assets().commitment()),
        assets_number_0 = p2id_note_0.assets().num_assets(),
        COMPUTED_ASSETS_COMMITMENT_1 = word_to_masm_push_string(&p2id_note_1.assets().commitment()),
        assets_number_1 = p2id_note_1.assets().num_assets(),
    );

    let tx_script = TransactionScript::compile(code, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::AccountId(account.id()),
            &[],
            &[p2id_note_0, p2id_note_1],
        )?
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

#[test]
fn test_input_note_get_recipient_and_metadata() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(crate::Auth::BasicAuth)?;
    let p2id_note_0 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(150)],
        NoteType::Public,
    )?;
    let p2id_note_1 = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[FungibleAsset::mock(300)],
        NoteType::Public,
    )?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        r#"
        use.miden::input_note

        begin
            ### 0'th note

            # get the recipient commitment from the 0'th input note
            push.0
            exec.input_note::get_recipient
            # => [RECIPIENT_0]

            # assert the correctness of the recipient
            push.{RECIPIENT_0} 
            assert_eqw.err="note 0 has incorrect recipient"
            # => []

            # get the metadata from the 0'th input note
            push.0
            exec.input_note::get_metadata
            # => [METADATA_0]

            # assert the correctness of the metadata
            push.{METADATA_0} 
            assert_eqw.err="note 0 has incorrect metadata"
            # => []

            ### 1'st note

            # get the recipient commitment from the 1'st input note
            push.1
            exec.input_note::get_recipient
            # => [RECIPIENT_1]

            # assert the correctness of the recipient
            push.{RECIPIENT_1} 
            assert_eqw.err="note 1 has incorrect recipient"
            # => []

            # get the metadata from the 1'st input note
            push.1
            exec.input_note::get_metadata
            # => [METADATA_1]

            # assert the correctness of the metadata
            push.{METADATA_1} 
            assert_eqw.err="note 1 has incorrect metadata"
            # => []
        end
    "#,
        RECIPIENT_0 = word_to_masm_push_string(&p2id_note_0.recipient().digest()),
        METADATA_0 = word_to_masm_push_string(&p2id_note_0.metadata().into()),
        RECIPIENT_1 = word_to_masm_push_string(&p2id_note_1.recipient().digest()),
        METADATA_1 = word_to_masm_push_string(&p2id_note_1.metadata().into()),
    );

    let tx_script = TransactionScript::compile(code, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::AccountId(account.id()),
            &[],
            &[p2id_note_0, p2id_note_1],
        )?
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}
