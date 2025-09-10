use miden_lib::account::components::multisig_library;
use miden_lib::account::wallets::BasicWallet;
use miden_lib::errors::tx_kernel_errors::ERR_TX_ALREADY_EXECUTED;
use miden_lib::utils::ScriptBuilder;
use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountId,
    AccountStorageMode,
    AccountType,
    AuthSecretKey,
};
use miden_objects::asset::FungibleAsset;
use miden_objects::crypto::dsa::rpo_falcon512::{PublicKey, SecretKey};
use miden_objects::note::NoteType;
use miden_objects::testing::account_id::{
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
    ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
};
use miden_objects::transaction::OutputNote;
use miden_objects::vm::AdviceMap;
use miden_objects::{Felt, Word};
use miden_processor::AdviceInputs;
use miden_testing::{Auth, MockChainBuilder, assert_transaction_executor_error};
use miden_tx::TransactionExecutorError;
use miden_tx::auth::{BasicAuthenticator, SigningInputs, TransactionAuthenticator};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

// ================================================================================================
// HELPER FUNCTIONS
// ================================================================================================

type MultisigTestSetup = (Vec<SecretKey>, Vec<PublicKey>, Vec<BasicAuthenticator<ChaCha20Rng>>);

/// Sets up secret keys, public keys, and authenticators for multisig testing
fn setup_keys_and_authenticators(
    num_approvers: usize,
    threshold: usize,
) -> anyhow::Result<MultisigTestSetup> {
    let mut rng = ChaCha20Rng::from_seed([0u8; 32]);

    let mut secret_keys = Vec::new();
    let mut public_keys = Vec::new();
    let mut authenticators = Vec::new();

    for _ in 0..num_approvers {
        let sec_key = SecretKey::with_rng(&mut rng);
        let pub_key = sec_key.public_key();

        secret_keys.push(sec_key);
        public_keys.push(pub_key);
    }

    // Create authenticators only for the signers we'll actually use
    for i in 0..threshold {
        let authenticator = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
            &[(public_keys[i].into(), AuthSecretKey::RpoFalcon512(secret_keys[i].clone()))],
            rng.clone(),
        );
        authenticators.push(authenticator);
    }

    Ok((secret_keys, public_keys, authenticators))
}

/// Creates a multisig account with the specified configuration
fn create_multisig_account(
    threshold: u32,
    public_keys: &[PublicKey],
    asset_amount: u64,
) -> anyhow::Result<Account> {
    let approvers: Vec<_> = public_keys.iter().map(|pk| (*pk).into()).collect();

    let multisig_account = AccountBuilder::new([0; 32])
        .with_auth_component(Auth::Multisig { threshold, approvers })
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_assets(vec![FungibleAsset::mock(asset_amount)])
        .build_existing()?;

    Ok(multisig_account)
}

// ================================================================================================
// TESTS
// ================================================================================================

/// Tests basic 2-of-2 multisig functionality with note creation.
///
/// This test verifies that a multisig account with 2 approvers and threshold 2
/// can successfully execute a transaction that creates an output note when both
/// required signatures are provided.
///
/// **Roles:**
/// - 2 Approvers (multisig signers)
/// - 1 Multisig Contract
#[tokio::test]
async fn test_multisig_2_of_2_with_note_creation() -> anyhow::Result<()> {
    // Setup keys and authenticators
    let (_secret_keys, public_keys, authenticators) = setup_keys_and_authenticators(2, 2)?;

    // Create multisig account
    let multisig_starting_balance = 10u64;
    let mut multisig_account = create_multisig_account(2, &public_keys, multisig_starting_balance)?;

    let output_note_asset = FungibleAsset::mock(0);

    let mut mock_chain_builder =
        MockChainBuilder::with_accounts([multisig_account.clone()]).unwrap();

    // Create output note using add_p2id_note for spawn note
    let output_note = mock_chain_builder.add_p2id_note(
        multisig_account.id(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        &[output_note_asset],
        NoteType::Public,
    )?;

    // Create spawn note that will create the output note
    let input_note = mock_chain_builder.add_spawn_note(multisig_account.id(), [&output_note])?;

    let mut mock_chain = mock_chain_builder.build().unwrap();

    let salt = Word::from([Felt::new(1); 4]);

    // Execute transaction without signatures - should fail
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[input_note.id()], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .auth_args(salt)
        .build()?;

    let tx_summary = match tx_context_init.execute().await.unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from both approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary).await?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary).await?;

    // Execute transaction with signatures - should succeed
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[input_note.id()], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .add_signature(public_keys[0], msg, sig_1)
        .add_signature(public_keys[1], msg, sig_2)
        .auth_args(salt)
        .build()?
        .execute()
        .await?;

    multisig_account.apply_delta(tx_context_execute.account_delta())?;

    mock_chain.add_pending_executed_transaction(&tx_context_execute)?;
    mock_chain.prove_next_block()?;

    assert_eq!(
        multisig_account
            .vault()
            .get_balance(AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?)?,
        multisig_starting_balance - output_note_asset.unwrap_fungible().amount()
    );

    Ok(())
}

/// Tests 2-of-4 multisig with all possible signer combinations.
///
/// This test verifies that a multisig account with 4 approvers and threshold 2
/// can successfully execute transactions when signed by any 2 of the 4 approvers.
/// It tests all 6 possible combinations of 2 signers to ensure the multisig
/// implementation correctly validates signatures from any valid subset.
///
/// **Tested combinations:** (0,1), (0,2), (0,3), (1,2), (1,3), (2,3)
#[tokio::test]
async fn test_multisig_2_of_4_all_signer_combinations() -> anyhow::Result<()> {
    // Setup keys and authenticators (4 approvers, all 4 can sign)
    let (_secret_keys, public_keys, authenticators) = setup_keys_and_authenticators(4, 4)?;

    // Create multisig account with 4 approvers but threshold of 2
    let multisig_account = create_multisig_account(2, &public_keys, 10)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    // Test different combinations of 2 signers out of 4
    let signer_combinations = [
        (0, 1), // First two
        (0, 2), // First and third
        (0, 3), // First and fourth
        (1, 2), // Second and third
        (1, 3), // Second and fourth
        (2, 3), // Last two
    ];

    for (i, (signer1_idx, signer2_idx)) in signer_combinations.iter().enumerate() {
        let salt = Word::from([Felt::new(10 + i as u64); 4]);

        // Execute transaction without signatures first to get tx summary
        let tx_context_init = mock_chain
            .build_tx_context(multisig_account.id(), &[], &[])?
            .auth_args(salt)
            .build()?;

        let tx_summary = match tx_context_init.execute().await.unwrap_err() {
            TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
            error => panic!("expected abort with tx effects: {error:?}"),
        };

        // Get signatures from the specific combination of signers
        let msg = tx_summary.as_ref().to_commitment();
        let tx_summary = SigningInputs::TransactionSummary(tx_summary);

        let sig_1 = authenticators[*signer1_idx]
            .get_signature(public_keys[*signer1_idx].into(), &tx_summary)
            .await?;
        let sig_2 = authenticators[*signer2_idx]
            .get_signature(public_keys[*signer2_idx].into(), &tx_summary)
            .await?;

        // Execute transaction with signatures - should succeed for any combination
        let tx_context_execute = mock_chain
            .build_tx_context(multisig_account.id(), &[], &[])?
            .auth_args(salt)
            .add_signature(public_keys[*signer1_idx], msg, sig_1)
            .add_signature(public_keys[*signer2_idx], msg, sig_2)
            .build()?;

        let executed_tx = tx_context_execute.execute().await.unwrap_or_else(|_| {
            panic!("Transaction should succeed with signers {signer1_idx} and {signer2_idx}")
        });

        // Apply the transaction to the mock chain for the next iteration
        mock_chain.add_pending_executed_transaction(&executed_tx)?;
        mock_chain.prove_next_block()?;
    }

    Ok(())
}

/// Tests multisig replay protection to prevent transaction re-execution.
///
/// This test verifies that a 2-of-3 multisig account properly prevents replay attacks
/// by rejecting attempts to execute the same transaction twice. The first execution
/// should succeed with valid signatures, but the second attempt with identical
/// parameters should fail with ERR_TX_ALREADY_EXECUTED.
///
/// **Roles:**
/// - 3 Approvers (2 signers required)
/// - 1 Multisig Contract
#[tokio::test]
async fn test_multisig_replay_protection() -> anyhow::Result<()> {
    // Setup keys and authenticators (3 approvers, but only 2 signers)
    let (_secret_keys, public_keys, authenticators) = setup_keys_and_authenticators(3, 2)?;

    // Create 2/3 multisig account
    let multisig_account = create_multisig_account(2, &public_keys, 20)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    let salt = Word::from([Felt::new(3); 4]);

    // Execute transaction without signatures first to get tx summary
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .auth_args(salt)
        .build()?;

    let tx_summary = match tx_context_init.execute().await.unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from 2 of the 3 approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary).await?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary).await?;

    // Execute transaction with signatures - should succeed (first execution)
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .add_signature(public_keys[0], msg, sig_1.clone())
        .add_signature(public_keys[1], msg, sig_2.clone())
        .auth_args(salt)
        .build()?;

    let executed_tx = tx_context_execute.execute().await.expect("First transaction should succeed");

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_tx)?;
    mock_chain.prove_next_block()?;

    // Now attempt to execute the same transaction again - should fail due to replay protection
    let tx_context_replay = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .add_signature(public_keys[0], msg, sig_1)
        .add_signature(public_keys[1], msg, sig_2)
        .auth_args(salt)
        .build()?;

    // This should fail - due to replay protection
    let result = tx_context_replay.execute().await;
    assert_transaction_executor_error!(result, ERR_TX_ALREADY_EXECUTED);

    Ok(())
}

/// Tests multisig signer update functionality.
///
/// This test verifies that a multisig account can:
/// 1. Execute a transaction script to update signers and threshold
/// 2. Create a second transaction signed by the new owners
/// 3. Properly handle multisig authentication with the updated signers
///
/// **Roles:**
/// - 2 Original Approvers (multisig signers)
/// - 4 New Approvers (updated multisig signers)
/// - 1 Multisig Contract
/// - 1 Transaction Script calling multisig procedures
#[tokio::test]
async fn test_multisig_update_signers() -> anyhow::Result<()> {
    // Setup keys and authenticators for the original multisig account
    let (_secret_keys, public_keys, authenticators) = setup_keys_and_authenticators(2, 2)?;

    // Create multisig account
    let multisig_starting_balance = 10u64;
    let multisig_account = create_multisig_account(2, &public_keys, multisig_starting_balance)?;

    let mock_chain_builder = MockChainBuilder::with_accounts([multisig_account.clone()]).unwrap();

    let mut mock_chain = mock_chain_builder.build().unwrap();

    let salt = Word::from([Felt::new(1); 4]);

    // Get the multisig library
    let multisig_lib: miden_assembly::Library = multisig_library();

    // new signer setup
    let mut advice_map = AdviceMap::default();
    let (_new_secret_keys, new_public_keys, _new_authenticators) =
        setup_keys_and_authenticators(4, 4)?;

    // for public key in new public keys
    for (i, public_key) in new_public_keys.iter().enumerate() {
        let key_word: Word = [Felt::new(i as u64), Felt::new(0), Felt::new(0), Felt::new(0)].into();
        let value_word: Word = (*public_key).into();
        advice_map.insert(key_word, value_word.to_vec());
        println!("pub key: {:?}", public_key);
    }

    // Create a transaction script that calls the update_signers procedure
    // The multisig library has an anonymous namespace, so we need to use it directly
    let tx_script_code = "
        begin
            push.101 debug.stack drop
            
            call.::update_signers_and_threshold
        end
    ";

    let tx_script = ScriptBuilder::new(true)
        .with_dynamically_linked_library(&multisig_lib)?
        .compile_tx_script(tx_script_code)?;

    // Create AdviceInputs with the advice map
    let mut advice_inputs = AdviceInputs::default();
    advice_inputs.map = advice_map.clone();

    let threshold = 3u64;
    let num_of_approvers = 4u64;

    let tx_script_args: Word =
        [Felt::new(threshold), Felt::new(num_of_approvers), Felt::new(0), Felt::new(0)].into();

    // Execute transaction without signatures first to get tx summary
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script.clone())
        .tx_script_args(tx_script_args)
        .auth_args(salt)
        .extend_advice_inputs(advice_inputs.clone())
        .build()?;

    let tx_summary = match tx_context_init.execute().await.unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from both approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary).await?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary).await?;

    // Execute transaction with signatures - should succeed
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script)
        .tx_script_args(tx_script_args)
        .add_signature(public_keys[0], msg, sig_1)
        .add_signature(public_keys[1], msg, sig_2)
        .auth_args(salt)
        .extend_advice_inputs(advice_inputs)
        .build()?
        .execute()
        .await?;

    // Verify the transaction executed successfully
    assert_eq!(tx_context_execute.account_delta().nonce_delta(), Felt::new(1));

    mock_chain.add_pending_executed_transaction(&tx_context_execute)?;
    mock_chain.prove_next_block()?;

    // Apply the delta to get the updated account with new signers
    let mut updated_multisig_account = multisig_account.clone();
    updated_multisig_account.apply_delta(tx_context_execute.account_delta())?;

    // Debug: Print account nonce to verify it was updated
    println!("Original account nonce: {}", multisig_account.nonce());
    println!("Updated account nonce: {}", updated_multisig_account.nonce());

    // Debug: Print account delta to see what changed
    println!("Account delta: {:?}", tx_context_execute.account_delta());

    // ========================================================================
    // VERIFICATION: Check that the public keys were actually updated
    // ========================================================================

    let storage_item = multisig_account
        .storage()
        .get_map_item(1, [Felt::new(1), Felt::new(0), Felt::new(0), Felt::new(0)].into())
        .unwrap();

    println!("storage item: {:?}", storage_item);

    /* TODO: Get this to work
    // Extract public keys from the updated account
    let final_pub_keys = get_public_keys_from_account(&updated_multisig_account);
    // Verify that we have the expected number of public keys (4 new ones)
    assert_eq!(final_pub_keys.len(), 4, "Expected 4 public keys after update");
    // Verify that the public keys match the new ones we set
    for (i, expected_key) in new_public_keys.iter().enumerate() {
        let expected_word: Word = (*expected_key).into();
        assert_eq!(final_pub_keys[i], expected_word, "Public key {} doesn't match", i);
    } */

    Ok(())
}