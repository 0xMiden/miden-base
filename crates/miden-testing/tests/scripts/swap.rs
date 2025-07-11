use anyhow::Context;
use miden_lib::{note::create_swap_note, transaction::TransactionKernel};
use miden_objects::{
    Felt, Word,
    account::{Account, AccountStorageMode, AccountType},
    asset::{Asset, FungibleAsset, NonFungibleAsset},
    crypto::rand::RpoRandomCoin,
    note::{Note, NoteDetails, NoteType},
    testing::account_id::AccountIdBuilder,
    transaction::{OutputNote, TransactionScript},
};
use miden_testing::{Auth, MockChain};
use miden_tx::utils::word_to_masm_push_string;

use crate::prove_and_verify_transaction;

/// Creates a SWAP note from the transaction script and proves and verifies the transaction.
#[test]
pub fn prove_send_swap_note() -> anyhow::Result<()> {
    let SwapTestSetup {
        mock_chain,
        mut sender_account,
        offered_asset,
        swap_note,
        ..
    } = setup_swap_test()?;

    // CREATE SWAP NOTE TX
    // --------------------------------------------------------------------------------------------

    let tx_script_src = &format!(
        "
        use.miden::tx
        begin
            push.{recipient}
            push.{note_execution_hint}
            push.{note_type}
            push.0              # aux
            push.{tag}
            call.tx::create_note

            push.{asset}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            dropw dropw dropw dropw
        end
        ",
        recipient = word_to_masm_push_string(&swap_note.recipient().digest()),
        note_type = NoteType::Public as u8,
        tag = Felt::from(swap_note.metadata().tag()),
        asset = word_to_masm_push_string(&offered_asset.into()),
        note_execution_hint = Felt::from(swap_note.metadata().execution_hint())
    );

    let tx_script =
        TransactionScript::compile(tx_script_src, TransactionKernel::testing_assembler()).unwrap();

    let create_swap_note_tx = mock_chain
        .build_tx_context(sender_account.id(), &[], &[])
        .context("failed to build tx context")?
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![OutputNote::Full(swap_note.clone())])
        .build()?
        .execute()?;

    sender_account
        .apply_delta(create_swap_note_tx.account_delta())
        .context("failed to apply delta")?;

    assert!(
        create_swap_note_tx
            .output_notes()
            .iter()
            .any(|n| n.commitment() == swap_note.commitment())
    );
    assert_eq!(
        sender_account.vault().assets().count(),
        0,
        "offered asset should no longer be present in vault"
    );

    let swap_output_note = create_swap_note_tx.output_notes().iter().next().unwrap();
    assert_eq!(swap_output_note.assets().unwrap().iter().next().unwrap(), &offered_asset);
    assert!(prove_and_verify_transaction(create_swap_note_tx).is_ok());

    Ok(())
}

/// Creates a SWAP note in the mock chain and consumes the created SWAP note, which creates a
/// payback note. The payback note is consumed by the original sender of the SWAP note.
///
/// Both transactions are proven and verified.
#[test]
fn prove_consume_swap_note() -> anyhow::Result<()> {
    let SwapTestSetup {
        mock_chain,
        mut sender_account,
        mut target_account,
        offered_asset,
        requested_asset,
        swap_note,
        payback_note,
    } = setup_swap_test()?;

    // CONSUME CREATED NOTE
    // --------------------------------------------------------------------------------------------

    let consume_swap_note_tx = mock_chain
        .build_tx_context(target_account.id(), &[swap_note.id()], &[])
        .context("failed to build tx context")?
        .build()?
        .execute()?;

    target_account
        .apply_delta(consume_swap_note_tx.account_delta())
        .context("failed to apply delta to target account")?;

    let output_payback_note = consume_swap_note_tx.output_notes().iter().next().unwrap().clone();
    assert!(output_payback_note.id() == payback_note.id());
    assert_eq!(output_payback_note.assets().unwrap().iter().next().unwrap(), &requested_asset);

    assert!(target_account.vault().assets().count() == 1);
    assert!(target_account.vault().assets().any(|asset| asset == offered_asset));

    // CONSUME PAYBACK P2ID NOTE
    // --------------------------------------------------------------------------------------------

    let full_payback_note = Note::new(
        payback_note.assets().clone(),
        *output_payback_note.metadata(),
        payback_note.recipient().clone(),
    );

    let consume_payback_tx = mock_chain
        .build_tx_context(sender_account.id(), &[], &[full_payback_note])
        .context("failed to build tx context")?
        .build()?
        .execute()?;

    sender_account
        .apply_delta(consume_payback_tx.account_delta())
        .context("failed to apply delta to sender account")?;

    assert!(sender_account.vault().assets().any(|asset| asset == requested_asset));

    prove_and_verify_transaction(consume_swap_note_tx)
        .context("failed to prove/verify consume_swap_note_tx")?;

    prove_and_verify_transaction(consume_payback_tx)
        .context("failed to prove/verify consume_payback_tx")?;

    Ok(())
}

struct SwapTestSetup {
    mock_chain: MockChain,
    sender_account: Account,
    target_account: Account,
    offered_asset: Asset,
    requested_asset: Asset,
    swap_note: Note,
    payback_note: NoteDetails,
}

fn setup_swap_test() -> anyhow::Result<SwapTestSetup> {
    let faucet_id = AccountIdBuilder::new()
        .account_type(AccountType::FungibleFaucet)
        .storage_mode(AccountStorageMode::Private)
        .build_with_seed([5; 32]);

    let offered_asset = FungibleAsset::new(faucet_id, 2000)?.into();
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let mut builder = MockChain::builder();
    let sender_account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, vec![offered_asset])?;
    let target_account =
        builder.add_existing_wallet_with_assets(Auth::BasicAuth, vec![requested_asset])?;
    let (swap_note, payback_note) = create_swap_note(
        sender_account.id(),
        offered_asset,
        requested_asset,
        NoteType::Public,
        Felt::new(0),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    builder.add_note(OutputNote::Full(swap_note.clone()));
    let mock_chain = builder.build()?;

    Ok(SwapTestSetup {
        mock_chain,
        sender_account,
        target_account,
        offered_asset,
        requested_asset,
        swap_note,
        payback_note,
    })
}
