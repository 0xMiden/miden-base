use alloc::string::String;

use miden_lib::note::create_p2id_note;
use miden_lib::utils::ScriptBuilder;
use miden_objects::Word;
use miden_objects::account::AccountId;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::crypto::rand::RpoRandomCoin;
use miden_objects::note::{Note, NoteType};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
};
use miden_objects::transaction::OutputNote;

use super::{Felt, TestSetup, setup_test, word_to_masm_push_string};
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
            # create an output note with fungible asset 0
            push.{RECIPIENT}
            push.{note_execution_hint}
            push.{note_type}
            push.0              # aux
            push.{tag}
            call.tx::create_note
            # => [note_idx]

            # move the asset 0 to the note
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
        // output note
        RECIPIENT = word_to_masm_push_string(&output_note_1.recipient().digest()),
        note_execution_hint = Felt::from(output_note_1.metadata().execution_hint()),
        note_type = NoteType::Public as u8,
        tag = output_note_1.metadata().tag(),
        asset_0 = word_to_masm_push_string(&fungible_asset_0.into()),
        // first data request
        COMPUTED_ASSETS_COMMITMENT_0 =
            word_to_masm_push_string(&output_note_0.assets().commitment()),
        assets_number_0 = output_note_0.assets().num_assets(),
        // second data request
        asset_1 = word_to_masm_push_string(&fungible_asset_1.into()),
        COMPUTED_ASSETS_COMMITMENT_1 =
            word_to_masm_push_string(&output_note_1.assets().commitment()),
        assets_number_1 = output_note_1.assets().num_assets(),
    );

    let tx_script = ScriptBuilder::with_kernel_library()?.compile_tx_script(tx_script_src)?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note_1)])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute_blocking()?;

    Ok(())
}

/// Check that recipient and metadata of a note with one asset obtained from the
/// `output_note::get_recipient` procedure is correct.
#[test]
fn test_get_recipient_and_metadata() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, [FungibleAsset::mock(2000)])?;

    let mock_chain = builder.build()?;

    let output_note = create_p2id_note(
        account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into()?,
        vec![FungibleAsset::mock(5)],
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )?;

    let tx_script_src = &format!(
        r#"
        use.miden::tx
        use.miden::output_note
        use.std::sys

        begin
            # create an output note with one asset
            {output_note} drop
            # => []

            # get the recipient (the only existing note has 0'th index)
            push.0
            exec.output_note::get_recipient
            # => [RECIPIENT]

            # assert the correctness of the recipient
            push.{RECIPIENT}
            assert_eqw.err="requested note has incorrect recipient"
            # => []

            # get the metadata (the only existing note has 0'th index)
            push.0
            exec.output_note::get_metadata
            # => [METADATA]

            # assert the correctness of the metadata
            push.{METADATA}
            assert_eqw.err="requested note has incorrect metadata"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        output_note = create_output_note(&output_note),
        RECIPIENT = word_to_masm_push_string(&output_note.recipient().digest()),
        METADATA = word_to_masm_push_string(&output_note.metadata().into()),
    );

    let tx_script = ScriptBuilder::with_kernel_library()?.compile_tx_script(tx_script_src)?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute_blocking()?;

    Ok(())
}

/// Check that the assets number and assets data obtained from the `output_note::get_assets`
/// procedure is correct for each note with zero, one and two different assets.
#[test]
fn test_get_assets() -> anyhow::Result<()> {
    let TestSetup {
        mock_chain,
        account,
        p2id_note_0_assets,
        p2id_note_1_asset,
        p2id_note_2_assets,
    } = setup_test()?;

    fn check_assets_code(note_index: u8, dest_ptr: u8, note: &Note) -> String {
        let mut check_assets_code = format!(
            r#"
            # push the note index and memory destination pointer
            push.{note_idx}.{dest_ptr}
            # => [dest_ptr, note_index]

            # write the assets to the memory
            exec.output_note::get_assets
            # => [num_assets, dest_ptr, note_index]

            # assert the number of note assets
            push.{assets_number}
            assert_eq.err="note {note_index} has incorrect assets number"
            # => [dest_ptr, note_index]
        "#,
            note_idx = note_index,
            dest_ptr = dest_ptr,
            assets_number = note.assets().num_assets(),
        );

        // check each asset in the note
        for (asset_index, asset) in note.assets().iter().enumerate() {
            check_assets_code.push_str(&format!(
                r#"
                    # load the asset stored in memory
                    padw dup.4 mem_loadw
                    # => [STORED_ASSET, dest_ptr, note_index]

                    # assert the asset
                    push.{NOTE_ASSET}
                    assert_eqw.err="asset {asset_index} of the note {note_index} is incorrect"
                    # => [dest_ptr, note_index]

                    # move the pointer
                    add.4
                    # => [dest_ptr+4, note_index]
                "#,
                NOTE_ASSET = word_to_masm_push_string(&asset.into()),
                asset_index = asset_index,
                note_index = note_index,
            ));
        }

        // drop the final `dest_ptr` and `note_index` from the stack
        check_assets_code.push_str("\ndrop drop");

        check_assets_code
    }

    let tx_script_src = &format!(
        "
        use.miden::tx
        use.miden::output_note
        use.std::sys

        begin
            {create_note_0}
            {check_note_0}

            {create_note_1}
            {check_note_1}

            {create_note_2}
            {check_note_2}

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        create_note_0 = create_output_note(&p2id_note_0_assets),
        check_note_0 = check_assets_code(0, 0, &p2id_note_0_assets),
        create_note_1 = create_output_note(&p2id_note_1_asset),
        check_note_1 = check_assets_code(1, 4, &p2id_note_1_asset),
        create_note_2 = create_output_note(&p2id_note_2_assets),
        check_note_2 = check_assets_code(2, 8, &p2id_note_2_assets),
    );

    let tx_script = ScriptBuilder::with_kernel_library()?.compile_tx_script(tx_script_src)?;

    let tx_context = mock_chain
        .build_tx_context(account.id(), &[], &[])?
        .extend_expected_output_notes(vec![
            OutputNote::Full(p2id_note_0_assets),
            OutputNote::Full(p2id_note_1_asset),
            OutputNote::Full(p2id_note_2_assets),
        ])
        .tx_script(tx_script)
        .build()?;

    tx_context.execute_blocking()?;

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

/// Returns a `masm` code which creates an output note and adds some assets to it.
///
/// Data for the created output note and moved assets is obtained from the provided note.
fn create_output_note(note: &Note) -> String {
    let mut create_note_code = format!(
        "
        # create an output note
        push.{RECIPIENT}
        push.{note_execution_hint}
        push.{note_type}
        push.0              # aux
        push.{tag}
        call.tx::create_note
        # => [note_idx]
    ",
        RECIPIENT = word_to_masm_push_string(&note.recipient().digest()),
        note_execution_hint = Felt::from(note.metadata().execution_hint()),
        note_type = note.metadata().note_type() as u8,
        tag = Felt::from(note.metadata().tag()),
    );

    for asset in note.assets().iter() {
        create_note_code.push_str(&format!(
            "
            # move the asset to the note
            push.{asset}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw
            # => [note_idx]
        ",
            asset = word_to_masm_push_string(&asset.into())
        ));
    }

    create_note_code
}
