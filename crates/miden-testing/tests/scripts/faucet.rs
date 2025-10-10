extern crate alloc;

use miden_lib::account::faucets::FungibleFaucetExt;
use miden_lib::errors::tx_kernel_errors::ERR_FUNGIBLE_ASSET_DISTRIBUTE_WOULD_CAUSE_MAX_SUPPLY_TO_BE_EXCEEDED;
use miden_lib::note::create_p2id_note;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::Account;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_objects::note::{NoteAssets, NoteExecutionHint, NoteId, NoteMetadata, NoteTag, NoteType};
use miden_objects::transaction::{ExecutedTransaction, OutputNote};
use miden_objects::{Felt, Word};
use miden_testing::{Auth, MockChain, assert_transaction_executor_error};

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
    let tx_script_code = create_mint_script_code(params);
    let tx_script = ScriptBuilder::default().compile_tx_script(tx_script_code)?;
    let tx_context = mock_chain.build_tx_context(faucet, &[], &[])?.tx_script(tx_script).build()?;

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
    let faucet = builder.add_existing_faucet(Auth::BasicAuth, "TST", 200, None)?;
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
    let faucet = builder.add_existing_faucet(Auth::BasicAuth, "TST", 200, None)?;
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
    let faucet = builder.add_existing_faucet(Auth::BasicAuth, "TST", 200, Some(100))?;

    let fungible_asset = FungibleAsset::new(faucet.id(), 100).unwrap();

    // need to create a note with the fungible asset to be burned
    let burn_note_script_code = "
        # burn the asset
        begin
            dropw

            # pad the stack before call
            padw padw padw padw
            # => [pad(16)]

            exec.::miden::active_note::get_assets drop
            mem_loadw
            # => [ASSET, pad(12)]

            call.::miden::contracts::faucets::basic_fungible::burn

            # truncate the stack
            dropw dropw dropw dropw
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

// TEST PUBLIC NOTE CREATION DURING NOTE CONSUMPTION
// ================================================================================================

/// Tests that a public note can be created during note consumption by fetching the note script
/// from the data store. This test verifies the functionality added in issue #1972.
///
/// The test creates a note that calls the faucet's `distribute` function to create a PUBLIC
/// P2ID output note. The P2ID script is fetched from the data store during transaction execution.
#[tokio::test]
async fn test_public_note_creation_with_script_from_datastore() -> anyhow::Result<()> {
    use miden_lib::note::well_known_note::WellKnownNote;
    use miden_objects::account::{AccountId, AccountIdVersion, AccountType, AccountStorageMode};
    use miden_objects::crypto::rand::RpoRandomCoin;
    use miden_objects::note::{Note, NoteInputs, NoteRecipient};

    let mut builder = MockChain::builder();
    let faucet = builder.add_existing_faucet(Auth::BasicAuth, "TST", 200, None)?;

    // Parameters for the PUBLIC P2ID note that will be created by the faucet
    let recipient_account_id = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let amount = Felt::new(75);
    let tag = NoteTag::for_public_use_case(0, 0, miden_objects::note::NoteExecutionMode::Local)?;
    let aux = Felt::new(27);
    let note_execution_hint = NoteExecutionHint::on_block_slot(5, 6, 7);
    let note_type = NoteType::Public; // PUBLIC note

    // Create the expected P2ID note using the helper function for debugging purposes
    let mut rng = RpoRandomCoin::new([Felt::from(0u32); 4].into());
    let expected_p2id_note = create_p2id_note(
        faucet.id(),
        recipient_account_id,
        vec![FungibleAsset::new(faucet.id(), amount.into())?.into()],
        note_type,
        aux,
        &mut rng,
    )?;
    
    // Extract recipient information from the expected note
    let p2id_recipient = expected_p2id_note.recipient();
    let p2id_script_root = p2id_recipient.script().root();
    let serial_num = p2id_recipient.serial_num();
    let target_account_suffix = recipient_account_id.suffix();
    let target_account_prefix = recipient_account_id.prefix().as_felt();
    
    println!("note commitment: {:?}", expected_p2id_note.inputs().commitment());
    println!("note recipient: {:?}", expected_p2id_note.recipient().digest());
    println!("note script root: {:?}", expected_p2id_note.script().root());
 
    let note_script_code = format!(
        "
            use.miden::note
            
            begin
                # Build recipient hash from SERIAL_NUM, SCRIPT_ROOT, and INPUTS_COMMITMENT
                push.{script_root}
                # => [SCRIPT_ROOT]
                
                push.{serial_num}
                # => [SERIAL_NUM, SCRIPT_ROOT]

                # Store P2ID inputs (target account ID) in memory at address 0
                # P2ID inputs are: [target.suffix(), target.prefix()]
                push.{target_suffix}.{target_prefix} push.0.0
                push.0 mem_storew dropw
                # Memory[0] = [0, 0, target_prefix, target_suffix]

                push.2 push.0
                # => [inputs_ptr, num_inputs, SERIAL_NUM, SCRIPT_ROOT]
                
                exec.note::build_recipient
                # => [RECIPIENT]
                
                # Now call distribute with the computed recipient
                push.{note_execution_hint}
                push.{note_type}
                push.{aux}
                push.{tag}
                push.{amount}
                # => [amount, tag, aux, note_type, execution_hint, RECIPIENT]

                call.::miden::contracts::faucets::basic_fungible::distribute
                # => [note_idx, pad(15)]

                # Truncate the stack
                dropw dropw dropw dropw
            end
            ",
        note_type = note_type as u8,
        target_suffix = target_account_suffix,
        target_prefix = target_account_prefix,
        script_root = p2id_script_root,
        serial_num = serial_num,
        aux = aux,
        tag = u32::from(tag),
        note_execution_hint = Felt::from(note_execution_hint),
        amount = amount,
    );

    // Create the trigger note that will call distribute
    let trigger_note_script = miden_lib::utils::ScriptBuilder::default()
        .compile_note_script(note_script_code)?;
    
    let serial_num = Word::from([1, 2, 3, 4u32]);
    let trigger_note_inputs = NoteInputs::new(vec![])?;
    let trigger_note_recipient = NoteRecipient::new(serial_num, trigger_note_script, trigger_note_inputs);
    let trigger_note_metadata = NoteMetadata::new(
        faucet.id(),
        NoteType::Private,
        NoteTag::for_local_use_case(0, 0)?,
        NoteExecutionHint::always(),
        Felt::new(0),
    )?;
    let trigger_note_assets = NoteAssets::new(vec![])?;
    let trigger_note = Note::new(trigger_note_assets, trigger_note_metadata, trigger_note_recipient);

    builder.add_output_note(OutputNote::Full(trigger_note.clone()));
    let mock_chain = builder.build()?;

    // Add the P2ID script to the data store so it can be fetched during transaction execution
    let p2id_script = WellKnownNote::P2ID.script();
    
    // Execute the transaction - this should fetch the P2ID script from the data store
    let executed_transaction = mock_chain
        .build_tx_context(faucet.id(), &[trigger_note.id()], &[])?
        .add_note_script(p2id_script)
        .build()?
        .execute()
        .await?;

    // Verify that a PUBLIC P2ID note was created
    assert_eq!(executed_transaction.output_notes().num_notes(), 1);
    let output_note = executed_transaction.output_notes().get_note(0);

    // Verify the output note is public
    assert_eq!(output_note.metadata().note_type(), NoteType::Public);
    
    // Verify the output note contains the minted fungible asset
    let expected_asset = FungibleAsset::new(faucet.id(), amount.into())?;
    let expected_asset_obj = Asset::from(expected_asset);
    assert!(output_note.assets().unwrap().iter().any(|asset| asset == &expected_asset_obj));

    // Verify the note was created by the faucet
    assert_eq!(output_note.metadata().sender(), faucet.id());

    // Verify nonce was incremented
    assert_eq!(executed_transaction.account_delta().nonce_delta(), Felt::new(1));

    Ok(())
}
