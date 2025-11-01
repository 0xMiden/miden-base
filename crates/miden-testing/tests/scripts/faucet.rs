extern crate alloc;

use core::slice;
use std::sync::Arc;

use miden_lib::account::faucets::{BasicFungibleFaucet, FungibleFaucetExt, NetworkFungibleFaucet};
use miden_lib::errors::tx_kernel_errors::ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED;
use miden_lib::note::WellKnownNote;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    Account,
    AccountId,
    AccountIdVersion,
    AccountStorageMode,
    AccountType,
};
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteId,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};
use miden_objects::transaction::{ExecutedTransaction, OutputNote};
use miden_objects::{Felt, Word};
use miden_testing::{Auth, MockChain, assert_transaction_executor_error};

use crate::scripts::swap::create_p2id_note_exact;
use crate::{get_note_with_fungible_asset_and_script, prove_and_verify_transaction};

// Shared test utilities for faucet tests
// ================================================================================================

/// Common test parameters for faucet tests
pub struct FaucetTestParams {
    pub recipient: Word,
    pub tag: NoteTag,
    pub aux: Felt,
    pub note_execution_hint: NoteExecutionHint,
    pub note_type: NoteType,
    pub amount: Felt,
}

/// Creates minting script code for fungible asset distribution
pub fn create_mint_script_code(params: &FaucetTestParams) -> String {
    format!(
        "
            begin
                # pad the stack before call
                push.0.0.0 padw

                push.{recipient}
                push.{note_execution_hint}
                push.{note_type}
                push.{aux}
                push.{tag}
                push.{amount}
                # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

                call.::miden::contracts::faucets::basic_fungible::distribute
                # => [note_idx, pad(15)]

                # truncate the stack
                dropw dropw dropw dropw
            end
            ",
        note_type = params.note_type as u8,
        recipient = params.recipient,
        aux = params.aux,
        tag = u32::from(params.tag),
        note_execution_hint = Felt::from(params.note_execution_hint),
        amount = params.amount,
    )
}

/// Executes a minting transaction with the given faucet and parameters
pub async fn execute_mint_transaction(
    mock_chain: &mut MockChain,
    faucet: Account,
    params: &FaucetTestParams,
) -> anyhow::Result<ExecutedTransaction> {
    let source_manager = Arc::new(DefaultSourceManager::default());
    let tx_script_code = create_mint_script_code(params);
    let tx_script = ScriptBuilder::with_source_manager(source_manager.clone())
        .compile_tx_script(tx_script_code)?;
    let tx_context = mock_chain
        .build_tx_context(faucet, &[], &[])?
        .tx_script(tx_script)
        .with_source_manager(source_manager)
        .build()?;

    Ok(tx_context.execute().await?)
}

/// Verifies minted output note matches expectations
pub fn verify_minted_output_note(
    executed_transaction: &ExecutedTransaction,
    faucet: &Account,
    params: &FaucetTestParams,
) -> anyhow::Result<()> {
    let fungible_asset: Asset = FungibleAsset::new(faucet.id(), params.amount.into())?.into();

    let output_note = executed_transaction.output_notes().get_note(0).clone();
    let assets = NoteAssets::new(vec![fungible_asset])?;
    let id = NoteId::new(params.recipient, assets.commitment());

    assert_eq!(output_note.id(), id);
    assert_eq!(
        output_note.metadata(),
        &NoteMetadata::new(
            faucet.id(),
            params.note_type,
            params.tag,
            params.note_execution_hint,
            params.aux
        )?
    );

    Ok(())
}

// TESTS MINT FUNGIBLE ASSET
// ================================================================================================

/// Tests that minting assets on an existing faucet succeeds.
#[tokio::test]
async fn minting_fungible_asset_on_existing_faucet_succeeds() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let faucet = builder.add_existing_basic_faucet(Auth::BasicAuth, "TST", 200, None)?;
    let mut mock_chain = builder.build()?;

    let params = FaucetTestParams {
        recipient: Word::from([0, 1, 2, 3u32]),
        tag: NoteTag::for_local_use_case(0, 0).unwrap(),
        aux: Felt::new(27),
        note_execution_hint: NoteExecutionHint::on_block_slot(5, 6, 7),
        note_type: NoteType::Private,
        amount: Felt::new(100),
    };

    params
        .tag
        .validate(params.note_type)
        .expect("note tag should support private notes");

    let executed_transaction =
        execute_mint_transaction(&mut mock_chain, faucet.clone(), &params).await?;
    verify_minted_output_note(&executed_transaction, &faucet, &params)?;

    Ok(())
}

#[tokio::test]
async fn faucet_contract_mint_fungible_asset_fails_exceeds_max_supply() -> anyhow::Result<()> {
    // CONSTRUCT AND EXECUTE TX (Failure)
    // --------------------------------------------------------------------------------------------
    let mut builder = MockChain::builder();
    let faucet = builder.add_existing_basic_faucet(Auth::BasicAuth, "TST", 200, None)?;
    let mock_chain = builder.build()?;

    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = Felt::new(4);
    let amount = Felt::new(250);

    let tx_script_code = format!(
        "
            begin
                # pad the stack before call
                push.0.0.0 padw

                push.{recipient}
                push.{note_type}
                push.{aux}
                push.{tag}
                push.{amount}
                # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

                call.::miden::contracts::faucets::basic_fungible::distribute
                # => [note_idx, pad(15)]

                # truncate the stack
                dropw dropw dropw dropw

            end
            ",
        note_type = NoteType::Private as u8,
        recipient = recipient,
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(tx_script_code)?;
    let tx = mock_chain
        .build_tx_context(faucet.id(), &[], &[])?
        .tx_script(tx_script)
        .build()?
        .execute()
        .await;

    // Execute the transaction and get the witness
    assert_transaction_executor_error!(
        tx,
        ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED
    );
    Ok(())
}

// TESTS FOR NEW FAUCET EXECUTION ENVIRONMENT
// ================================================================================================

/// Tests that minting assets on a new faucet succeeds.
#[tokio::test]
async fn minting_fungible_asset_on_new_faucet_succeeds() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let faucet = builder.create_new_faucet(Auth::BasicAuth, "TST", 200)?;
    let mut mock_chain = builder.build()?;

    let params = FaucetTestParams {
        recipient: Word::from([0, 1, 2, 3u32]),
        tag: NoteTag::for_local_use_case(0, 0).unwrap(),
        aux: Felt::new(27),
        note_execution_hint: NoteExecutionHint::on_block_slot(5, 6, 7),
        note_type: NoteType::Private,
        amount: Felt::new(100),
    };

    params
        .tag
        .validate(params.note_type)
        .expect("note tag should support private notes");

    let executed_transaction =
        execute_mint_transaction(&mut mock_chain, faucet.clone(), &params).await?;
    verify_minted_output_note(&executed_transaction, &faucet, &params)?;

    Ok(())
}

// TESTS BURN FUNGIBLE ASSET
// ================================================================================================

/// Tests that burning a fungible asset on an existing faucet succeeds and proves the transaction.
#[tokio::test]
async fn prove_burning_fungible_asset_on_existing_faucet_succeeds() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let faucet = builder.add_existing_basic_faucet(Auth::BasicAuth, "TST", 200, Some(100))?;

    let fungible_asset = FungibleAsset::new(faucet.id(), 100).unwrap();

    // need to create a note with the fungible asset to be burned
    let burn_note_script_code = "
        # burn the asset
        begin
            dropw
            # => []

            call.::miden::contracts::faucets::basic_fungible::burn
            # => [ASSET]

            # truncate the stack
            dropw
        end
        ";

    let note = get_note_with_fungible_asset_and_script(fungible_asset, burn_note_script_code);

    builder.add_output_note(OutputNote::Full(note.clone()));
    let mock_chain = builder.build()?;

    // The Fungible Faucet component is added as the second component after auth, so it's storage
    // slot offset will be 2. Check that max_supply at the word's index 0 is 200. The remainder of
    // the word is initialized with the metadata of the faucet which we don't need to check.
    assert_eq!(faucet.storage().get_item(2).unwrap()[0], Felt::new(200));

    // Check that the faucet reserved slot has been correctly initialized.
    // The already issued amount should be 100.
    assert_eq!(faucet.get_token_issuance().unwrap(), Felt::new(100));

    // CONSTRUCT AND EXECUTE TX (Success)
    // --------------------------------------------------------------------------------------------
    // Execute the transaction and get the witness
    let executed_transaction = mock_chain
        .build_tx_context(faucet.id(), &[note.id()], &[])?
        .build()?
        .execute()
        .await?;

    // Prove, serialize/deserialize and verify the transaction
    prove_and_verify_transaction(executed_transaction.clone())?;

    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), note.id());
    Ok(())
}

// TESTS NETWORK FAUCET
// ================================================================================================

/// Tests minting on network faucet
#[tokio::test]
async fn network_faucet_mint() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let faucet_owner_account_id = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let faucet =
        builder.add_existing_network_faucet("NET", 1000, faucet_owner_account_id, Some(50))?;

    // Create a target account to consume the minted note
    let mut target_account = builder.add_existing_wallet(Auth::IncrNonce)?;

    // The Network Fungible Faucet component is added as the second component after auth, so its
    // storage slot offset will be 2. Check that max_supply at the word's index 0 is 200.
    assert_eq!(faucet.storage().get_item(1).unwrap()[0], Felt::new(1000));

    // Check that the creator account ID is stored in slot 2 (second storage slot of the component)
    // The owner_account_id is stored as Word [0, 0, suffix, prefix]
    let stored_owner_id = faucet.storage().get_item(2).unwrap();
    assert_eq!(stored_owner_id[3], faucet_owner_account_id.prefix().as_felt());
    assert_eq!(stored_owner_id[2], Felt::new(faucet_owner_account_id.suffix().as_int()));

    // Check that the faucet reserved slot has been correctly initialized.
    // The already issued amount should be 50.
    assert_eq!(faucet.get_token_issuance().unwrap(), Felt::new(50));

    // CREATE MINT NOTE USING STANDARD NOTE
    // --------------------------------------------------------------------------------------------

    let amount = Felt::new(75);
    let mint_asset: Asset = FungibleAsset::new(faucet.id(), amount.into()).unwrap().into();
    let tag = NoteTag::for_local_use_case(0, 0).unwrap();
    let aux = Felt::new(27);
    let note_execution_hint = NoteExecutionHint::on_block_slot(5, 6, 7);
    let note_type = NoteType::Private;
    let serial_num = Word::default();

    let p2id_mint_output_note = create_p2id_note_exact(
        faucet.id(),
        target_account.id(),
        vec![mint_asset],
        note_type,
        aux,
        serial_num,
    )
    .unwrap();
    let recipient = p2id_mint_output_note.recipient().digest();

    // Use the standard MINT note script
    let note_script = WellKnownNote::MINT.script();

    // Create the note inputs for MINT note (reversed order)
    let inputs = NoteInputs::new(vec![
        recipient[0],
        recipient[1],
        recipient[2],
        recipient[3],
        note_execution_hint.into(),
        note_type.into(),
        aux,
        tag.into(),
        amount,
    ])?;

    // Create the MINT note using the standard script
    let mint_note_metadata =
        NoteMetadata::new(faucet_owner_account_id, note_type, tag, note_execution_hint, aux)?;
    let mint_note_assets = NoteAssets::new(vec![])?; // Empty assets for mint note
    let serial_num = Word::from([1, 2, 3, 4u32]); // Random serial number
    let mint_note_recipient = NoteRecipient::new(serial_num, note_script, inputs);
    let mint_note = Note::new(mint_note_assets, mint_note_metadata, mint_note_recipient);

    // Add the MINT note to the mock chain
    builder.add_output_note(OutputNote::Full(mint_note.clone()));
    let mut mock_chain = builder.build()?;

    // EXECUTE MINT NOTE AGAINST NETWORK FAUCET
    // --------------------------------------------------------------------------------------------
    let tx_context = mock_chain.build_tx_context(faucet.id(), &[mint_note.id()], &[])?.build()?;
    let executed_transaction = tx_context.execute().await?;

    // Check that a P2ID note was created by the faucet
    assert_eq!(executed_transaction.output_notes().num_notes(), 1);
    let output_note = executed_transaction.output_notes().get_note(0);

    // Verify the output note contains the minted fungible asset
    let expected_asset = FungibleAsset::new(faucet.id(), amount.into())?;
    let assets = NoteAssets::new(vec![expected_asset.into()])?;
    let expected_note_id = NoteId::new(recipient, assets.commitment());

    assert_eq!(output_note.id(), expected_note_id);
    assert_eq!(output_note.metadata().sender(), faucet.id());

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_transaction)?;
    mock_chain.prove_next_block()?;

    // CONSUME THE OUTPUT NOTE WITH TARGET ACCOUNT
    // --------------------------------------------------------------------------------------------
    // Execute transaction to consume the output note with the target account
    let consume_tx_context = mock_chain
        .build_tx_context(target_account.id(), &[], slice::from_ref(&p2id_mint_output_note))?
        .build()?;
    let consume_executed_transaction = consume_tx_context.execute().await?;

    // Apply the delta to the target account and verify the asset was added to the account's vault
    target_account.apply_delta(consume_executed_transaction.account_delta())?;

    // Verify the account's vault now contains the expected fungible asset
    let balance = target_account.vault().get_balance(faucet.id())?;
    assert_eq!(balance, expected_asset.amount(),);

    Ok(())
}

// TESTS FOR FAUCET PROCEDURE COMPATIBILITY
// ================================================================================================

/// Tests that basic and network fungible faucets have the same burn procedure digest.
/// This is required for BURN notes to work with both faucet types.
#[test]
fn test_faucet_burn_procedures_are_identical() {
    // Both faucet types must export the same burn procedure with identical MAST roots
    // so that a single BURN note script can work with either faucet type
    assert_eq!(
        BasicFungibleFaucet::burn_digest(),
        NetworkFungibleFaucet::burn_digest(),
        "Basic and network fungible faucets must have the same burn procedure digest"
    );
}

/// Tests burning on network faucet
#[tokio::test]
async fn network_faucet_burn() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();

    let faucet_owner_account_id = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let mut faucet =
        builder.add_existing_network_faucet("NET", 200, faucet_owner_account_id, Some(100))?;

    let burn_amount = 100u64;
    let fungible_asset = FungibleAsset::new(faucet.id(), burn_amount).unwrap();

    // CREATE BURN NOTE USING STANDARD NOTE SCRIPT
    // --------------------------------------------------------------------------------------------
    // Use the standard BURN note script
    let note_script = WellKnownNote::BURN.script();

    // Create the burn note using the standard script
    let burn_note_metadata = NoteMetadata::new(
        faucet_owner_account_id,
        NoteType::Public,
        NoteTag::for_local_use_case(0, 0)?,
        NoteExecutionHint::Always,
        Felt::new(0),
    )?;
    let burn_note_assets = NoteAssets::new(vec![fungible_asset.into()])?;
    let serial_num = Word::from([5, 6, 7, 8u32]);
    let inputs = NoteInputs::new(vec![]).unwrap();
    let burn_note_recipient = NoteRecipient::new(serial_num, note_script, inputs);
    let note = Note::new(burn_note_assets, burn_note_metadata, burn_note_recipient);

    builder.add_output_note(OutputNote::Full(note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    // Check the initial token issuance before burning
    let initial_issuance = faucet.get_token_issuance().unwrap();
    assert_eq!(initial_issuance, Felt::new(100));

    // EXECUTE BURN NOTE AGAINST NETWORK FAUCET
    // --------------------------------------------------------------------------------------------
    let tx_context = mock_chain.build_tx_context(faucet.id(), &[note.id()], &[])?.build()?;
    let executed_transaction = tx_context.execute().await?;

    // Check that the burn was successful - no output notes should be created for burn
    assert_eq!(executed_transaction.output_notes().num_notes(), 0);

    // Verify the transaction was executed successfully
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));
    assert_eq!(executed_transaction.input_notes().get_note(0).id(), note.id());

    // Apply the delta to the faucet account and verify the token issuance decreased
    faucet.apply_delta(executed_transaction.account_delta())?;
    let final_issuance = faucet.get_token_issuance().unwrap();
    assert_eq!(final_issuance, Felt::new(initial_issuance.as_int() - burn_amount));

    Ok(())
}
