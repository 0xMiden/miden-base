use anyhow::Context;
use miden_lib::{note::create_p2id_note, transaction::TransactionKernel};
use miden_objects::{
    Word,
    account::AccountId,
    asset::{Asset, FungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::NoteType,
    testing::account_id::{
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
    },
    transaction::{OutputNote, TransactionScript},
};

use super::{Felt, word_to_masm_push_string};
use crate::{Auth, MockChain};

/// This test creates an output note and then adds some assets into it checking the assets info on
/// each stage.
///
/// Namely, we invoke the `miden::output_notes::get_assets_info` procedure:
/// - After adding the first `asset_0` to the note.
/// - Right after the previous check to make sure it returns the same commitment from the cached
///   data.
/// - After adding the second `asset_1` to the note.
#[test]
fn test_get_asset_info() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let fungible_asset_0 = Asset::Fungible(
        FungibleAsset::new(
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).expect("id should be valid"),
            5,
        )
        .expect("asset is invalid"),
    );

    // create the second asset with the different faucet ID to increase the number of assets in the
    // output note to 2.
    let fungible_asset_1 = Asset::Fungible(
        FungibleAsset::new(
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).expect("id should be valid"),
            5,
        )
        .expect("asset is invalid"),
    );

    let account = builder
        .add_existing_wallet_with_assets(Auth::BasicAuth, [fungible_asset_0, fungible_asset_1])?;

    let mock_chain = builder.build()?;

    let output_note_0 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![fungible_asset_0],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    let output_note_1 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![fungible_asset_0, fungible_asset_1],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([4, 3, 2, 1u32])),
    )?;

    let tx_script_src = &format!(
        r#"
        use.miden::tx
        use.miden::output_note
        use.std::sys

        begin
            # create an output note
            push.{recipient}
            push.{note_execution_hint}
            push.{note_type}
            push.0              # aux
            push.{tag}
            exec.tx::create_note
            # => [note_idx]

            # add asset_0 to the note
            push.{asset_0}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw
            # => [note_idx]

            # get the assets hash and assets number of the note having only asset_0
            dup exec.output_note::get_assets_info
            # => [ASSETS_COMMITMENT_0, num_assets_0, note_idx]

            # assert the correctness of the assets hash
            push.{COMPUTED_ASSETS_COMMITMENT_0}
            assert_eqw.err="assets commitment of the note having only asset_0 is incorrect"
            # => [num_assets_0, note_idx]

            # assert the number of assets
            push.{assets_number_0}
            assert_eq.err="number of assets in the note having only asset_0 is incorrect"
            # => [note_idx]

            # get the assets info once more to get the cached data and assert that this data didn't
            # change
            dup exec.output_note::get_assets_info
            push.{COMPUTED_ASSETS_COMMITMENT_0}
            assert_eqw.err="assets commitment of the note having only asset_0 is incorrect"
            push.{assets_number_0}
            assert_eq.err="number of assets in the note having only asset_0 is incorrect"
            # => [note_idx]

            # add asset_1 to the note
            push.{asset_1}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw
            # => [note_idx]

            # get the assets hash and assets number of the note having asset_0 and asset_1
            dup exec.output_note::get_assets_info
            # => [ASSETS_COMMITMENT_1, num_assets_1, note_idx]

            # assert the correctness of the assets hash
            push.{COMPUTED_ASSETS_COMMITMENT_1}
            assert_eqw.err="assets commitment of the note having asset_0 and asset_1 is incorrect"
            # => [num_assets_1, note_idx]

            # assert the number of assets
            push.{assets_number_1}
            assert_eq.err="number of assets in the note having asset_0 and asset_1 is incorrect"
            # => [note_idx]

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        // note data
        recipient = word_to_masm_push_string(&output_note_1.recipient().digest()),
        note_execution_hint = Felt::from(output_note_1.metadata().execution_hint()),
        note_type = NoteType::Public as u8,
        tag = Felt::from(output_note_1.metadata().tag()),
        // first data request
        asset_0 = word_to_masm_push_string(&fungible_asset_0.into()),
        COMPUTED_ASSETS_COMMITMENT_0 =
            word_to_masm_push_string(&output_note_0.assets().commitment()),
        assets_number_0 = output_note_0.assets().num_assets(),
        // second data request
        asset_1 = word_to_masm_push_string(&fungible_asset_1.into()),
        COMPUTED_ASSETS_COMMITMENT_1 =
            word_to_masm_push_string(&output_note_1.assets().commitment()),
        assets_number_1 = output_note_1.assets().num_assets(),
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

#[test]
fn test_get_recipient_and_metadata() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, [FungibleAsset::mock(2000)])?;

    let mock_chain = builder.build()?;

    let output_note_0 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    let output_note_1 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(5)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([4, 3, 2, 1u32])),
    )?;

    let tx_script_src = &format!(
        r#"
        use.miden::tx
        use.miden::output_note
        use.std::sys

        begin
            ### 0'th note

            # create output note 0
            push.{RECIPIENT_0}
            push.{note_execution_hint_0}
            push.{note_type_0}
            push.0              # aux
            push.{tag_0}
            call.tx::create_note
            # => [note_idx_0]

            # move asset_0 to the note 0
            push.{asset_0}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw drop
            # => []

            # get the recipient commitment from the 0'th output note
            push.0
            exec.output_note::get_recipient
            # => [RECIPIENT_0]

            # assert the correctness of the recipient
            push.{RECIPIENT_0} 
            assert_eqw.err="note 0 has incorrect recipient"
            # => []

            # get the metadata from the 0'th output note
            push.0
            exec.output_note::get_metadata
            # => [METADATA_0]

            # assert the correctness of the metadata
            push.{METADATA_0} 
            assert_eqw.err="note 0 has incorrect metadata"
            # => []

            ### 1'st note

            # create output note 1
            push.{RECIPIENT_1}
            push.{note_execution_hint_1}
            push.{note_type_1}
            push.0              # aux
            push.{tag_1}
            call.tx::create_note
            # => [note_idx_1]

            # move asset_1 to the note 1
            push.{asset_1}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw drop
            # => []

            # get the recipient commitment from the 1'st output note
            push.1
            exec.output_note::get_recipient
            # => [RECIPIENT_1]

            # assert the correctness of the recipient
            push.{RECIPIENT_1} 
            assert_eqw.err="note 1 has incorrect recipient"
            # => []

            # get the metadata from the 1'st output note
            push.1
            exec.output_note::get_metadata
            # => [METADATA_1]

            # assert the correctness of the metadata
            push.{METADATA_1} 
            assert_eqw.err="note 1 has incorrect metadata"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        // first note
        RECIPIENT_0 = word_to_masm_push_string(&output_note_0.recipient().digest()),
        note_execution_hint_0 = Felt::from(output_note_0.metadata().execution_hint()),
        note_type_0 = NoteType::Public as u8,
        tag_0 = Felt::from(output_note_0.metadata().tag()),
        asset_0 = word_to_masm_push_string(&FungibleAsset::mock(10).into()),
        METADATA_0 = word_to_masm_push_string(&output_note_0.metadata().into()),
        // second note
        RECIPIENT_1 = word_to_masm_push_string(&output_note_1.recipient().digest()),
        note_execution_hint_1 = Felt::from(output_note_1.metadata().execution_hint()),
        note_type_1 = NoteType::Public as u8,
        tag_1 = Felt::from(output_note_1.metadata().tag()),
        asset_1 = word_to_masm_push_string(&FungibleAsset::mock(5).into()),
        METADATA_1 = word_to_masm_push_string(&output_note_1.metadata().into()),
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(output_note_0),
            OutputNote::Full(output_note_1),
        ])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

#[test]
fn test_get_assets() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, [FungibleAsset::mock(2000)])?;

    let mock_chain = builder.build()?;

    let output_note_0 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;
    let note_0_asset = output_note_0
        .assets()
        .iter()
        .next()
        .context("output_note_0 should have at least one asset")?;

    let output_note_1 = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(5)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([4, 3, 2, 1u32])),
    )?;
    let note_1_asset = output_note_1
        .assets()
        .iter()
        .next()
        .context("output_note_1 should have at least one asset")?;

    let tx_script_src = &format!(
        r#"
        use.miden::tx
        use.miden::output_note
        use.std::sys

        begin
            ### 0'th note

            # create output note 0
            push.{RECIPIENT_0}
            push.{note_execution_hint_0}
            push.{note_type_0}
            push.0              # aux
            push.{tag_0}
            call.tx::create_note
            # => [note_idx_0]

            # move asset_0 to the note 0
            push.{asset_0}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw drop
            # => []

            # push the note index and memory pointer
            push.0.0
            # => [note_index, dest_ptr]

            # write the assets to the memory
            exec.output_note::get_assets
            # => [num_assets, note_index, dest_ptr]

            # assert the number of note assets
            push.{assets_number_0}
            assert_eq.err="note 0 has incorrect assets number"
            # => [note_index, dest_ptr]

            # assert the asset stored in memory
            drop mem_loadw
            # => [STORED_ASSET_0]

            # assert the asset
            push.{NOTE_0_ASSET}
            assert_eqw.err="note 0 has incorrect asset"
            # => []

            ### 1'st note

            # create output note 1
            push.{RECIPIENT_1}
            push.{note_execution_hint_1}
            push.{note_type_1}
            push.0              # aux
            push.{tag_1}
            call.tx::create_note
            # => [note_idx_1]

            # move asset_1 to the note 1
            push.{asset_1}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw drop
            # => []

            # push the note index and memory pointer
            push.4.1
            # => [note_index, dest_ptr]

            # write the assets to the memory
            exec.output_note::get_assets
            # => [num_assets, note_index, dest_ptr]

            # assert the number of note assets
            push.{assets_number_1}
            assert_eq.err="note 1 has incorrect assets number"
            # => [note_index, dest_ptr]

            # assert the asset stored in memory
            drop mem_loadw
            # => [STORED_ASSET_1]

            # assert the asset
            push.{NOTE_1_ASSET}
            assert_eqw.err="note 1 has incorrect asset"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        // first note
        RECIPIENT_0 = word_to_masm_push_string(&output_note_0.recipient().digest()),
        note_execution_hint_0 = Felt::from(output_note_0.metadata().execution_hint()),
        note_type_0 = NoteType::Public as u8,
        tag_0 = Felt::from(output_note_0.metadata().tag()),
        asset_0 = word_to_masm_push_string(&FungibleAsset::mock(10).into()),
        assets_number_0 = output_note_0.assets().num_assets(),
        NOTE_0_ASSET = word_to_masm_push_string(&note_0_asset.into()),
        // second note
        RECIPIENT_1 = word_to_masm_push_string(&output_note_1.recipient().digest()),
        note_execution_hint_1 = Felt::from(output_note_1.metadata().execution_hint()),
        note_type_1 = NoteType::Public as u8,
        tag_1 = Felt::from(output_note_1.metadata().tag()),
        asset_1 = word_to_masm_push_string(&FungibleAsset::mock(5).into()),
        assets_number_1 = output_note_1.assets().num_assets(),
        NOTE_1_ASSET = word_to_masm_push_string(&note_1_asset.into()),
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(output_note_0),
            OutputNote::Full(output_note_1),
        ])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}
