use alloc::string::String;

use anyhow::Context;
use miden_lib::transaction::TransactionKernel;
use miden_objects::{
    Word,
    account::{Account, AccountId},
    asset::{Asset, FungibleAsset},
    note::{Note, NoteType},
    testing::account_id::{
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1, ACCOUNT_ID_SENDER,
    },
    transaction::TransactionScript,
};

use super::word_to_masm_push_string;
use crate::{MockChain, TxContextInput};

/// Check that the assets number and assets commitment obtained from the
/// `input_note::get_assets_info` procedure is correct for each note with one, two and three
/// different assets.
#[test]
fn test_get_asset_info() -> anyhow::Result<()> {
    let TestSetup {
        mock_chain,
        account,
        p2id_note_0_assets,
        p2id_note_1_asset,
        p2id_note_2_assets,
    } = setup_test()?;

    fn check_asset_info_code(
        note_index: u8,
        assets_commitment: Word,
        assets_number: usize,
    ) -> String {
        format!(
            r#"
            # get the assets hash and assets number from the requested input note
            push.{note_index}
            exec.input_note::get_assets_info
            # => [ASSETS_COMMITMENT, num_assets]

            # assert the correctness of the assets hash
            push.{COMPUTED_ASSETS_COMMITMENT} 
            assert_eqw.err="note {note_index} has incorrect assets hash"
            # => [num_assets]

            # assert the number of note assets
            push.{assets_number}
            assert_eq.err="note {note_index} has incorrect assets number"
            # => []
        "#,
            note_index = note_index,
            COMPUTED_ASSETS_COMMITMENT = word_to_masm_push_string(&assets_commitment),
            assets_number = assets_number,
        )
    }

    let code = format!(
        r#"
        use.miden::input_note

        begin
            {check_note_0}

            {check_note_1}

            {check_note_2}
        end
    "#,
        check_note_0 = check_asset_info_code(
            0,
            p2id_note_0_assets.assets().commitment(),
            p2id_note_0_assets.assets().num_assets()
        ),
        check_note_1 = check_asset_info_code(
            1,
            p2id_note_1_asset.assets().commitment(),
            p2id_note_1_asset.assets().num_assets()
        ),
        check_note_2 = check_asset_info_code(
            2,
            p2id_note_2_assets.assets().commitment(),
            p2id_note_2_assets.assets().num_assets()
        ),
    );

    let tx_script = TransactionScript::compile(code, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::AccountId(account.id()),
            &[],
            &[p2id_note_0_assets, p2id_note_1_asset, p2id_note_2_assets],
        )?
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

/// Check that recipient and metadata of a note with one asset obtained from the
/// `input_note::get_recipient` procedure is correct.
#[test]
fn test_get_recipient_and_metadata() -> anyhow::Result<()> {
    let TestSetup {
        mock_chain,
        account,
        p2id_note_0_assets: _,
        p2id_note_1_asset,
        p2id_note_2_assets: _,
    } = setup_test()?;

    let code = format!(
        r#"
        use.miden::input_note

        begin
            # get the recipient from the input note
            push.0
            exec.input_note::get_recipient
            # => [RECIPIENT]

            # assert the correctness of the recipient
            push.{RECIPIENT} 
            assert_eqw.err="note 0 has incorrect recipient"
            # => []

            # get the metadata from the requested input note
            push.0
            exec.input_note::get_metadata
            # => [METADATA]

            # assert the correctness of the metadata
            push.{METADATA} 
            assert_eqw.err="note 0 has incorrect metadata"
            # => []
        end
    "#,
        RECIPIENT = word_to_masm_push_string(&p2id_note_1_asset.recipient().digest()),
        METADATA = word_to_masm_push_string(&p2id_note_1_asset.metadata().into()),
    );

    let tx_script = TransactionScript::compile(code, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(TxContextInput::AccountId(account.id()), &[], &[p2id_note_1_asset])?
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

/// Check that the assets number and assets data obtained from the `input_note::get_assets`
/// procedure is correct for each note with one, two and three different assets.
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
            push.{dest_ptr}.{note_idx}
            # => [note_index, dest_ptr]

            # write the assets to the memory
            exec.input_note::get_assets
            # => [num_assets, note_index, dest_ptr]

            # assert the number of note assets
            push.{assets_number}
            assert_eq.err="note {note_index} has incorrect assets number"
            drop
            # => [dest_ptr]
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
                    # => [STORED_ASSET, dest_ptr]

                    # assert the asset
                    push.{NOTE_ASSET}
                    assert_eqw.err="asset {asset_index} of the note {note_index} is incorrect"
                    # => [dest_ptr]

                    # move the pointer
                    add.4
                    # => [dest_ptr+4]
                "#,
                NOTE_ASSET = word_to_masm_push_string(&asset.into()),
                asset_index = asset_index,
                note_index = note_index,
            ));
        }

        // drop the final `dest_ptr` from the stack
        check_assets_code.push_str("\ndrop");

        check_assets_code
    }

    let code = format!(
        "
        use.miden::input_note

        begin
            {check_note_0}
            
            {check_note_1}

            {check_note_2}
        end
    ",
        check_note_0 = check_assets_code(0, 0, &p2id_note_0_assets),
        check_note_1 = check_assets_code(1, 4, &p2id_note_1_asset),
        check_note_2 = check_assets_code(2, 8, &p2id_note_2_assets),
    );

    let tx_script = TransactionScript::compile(code, TransactionKernel::testing_assembler())?;

    let tx_context = mock_chain
        .build_tx_context(
            TxContextInput::AccountId(account.id()),
            &[],
            &[p2id_note_0_assets, p2id_note_1_asset, p2id_note_2_assets],
        )?
        .tx_script(tx_script)
        .build()?;

    tx_context.execute()?;

    Ok(())
}

// HELPER STRUCTURE
// ================================================================================================

/// Helper struct which holds the data required for the `input_note` tests.
struct TestSetup {
    mock_chain: MockChain,
    account: Account,
    p2id_note_0_assets: Note,
    p2id_note_1_asset: Note,
    p2id_note_2_assets: Note,
}

/// Return a [`TestSetup`], whose notes contain 0, 1 and 2 assets respectively.
fn setup_test() -> anyhow::Result<TestSetup> {
    let mut builder = MockChain::builder();
    let account = builder.add_existing_wallet(crate::Auth::BasicAuth)?;

    // Assets
    let fungible_asset_0 = Asset::Fungible(
        FungibleAsset::new(
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).context("id should be valid")?,
            5,
        )
        .context("fungible_asset_0 is invalid")?,
    );
    let fungible_asset_1 = Asset::Fungible(
        FungibleAsset::new(
            AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1)
                .context("id should be valid")?,
            10,
        )
        .context("fungible_asset_1 is invalid")?,
    );

    // Notes
    let p2id_note_0_assets = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[],
        NoteType::Public,
    )?;
    let p2id_note_1_asset = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[fungible_asset_0],
        NoteType::Public,
    )?;
    let p2id_note_2_assets = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into().unwrap(),
        account.id(),
        &[fungible_asset_0, fungible_asset_1],
        NoteType::Public,
    )?;
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    anyhow::Ok(TestSetup {
        mock_chain,
        account,
        p2id_note_0_assets,
        p2id_note_1_asset,
        p2id_note_2_assets,
    })
}
