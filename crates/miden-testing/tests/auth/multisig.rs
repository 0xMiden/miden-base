use miden_lib::account::wallets::BasicWallet;
use miden_lib::errors::tx_kernel_errors::ERR_TX_ALREADY_EXECUTED;
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
use miden_objects::{Felt, Word};
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
    let input_note = mock_chain_builder.add_spawn_note([&output_note])?;

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

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary).await?.to_prepared_signature();
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary).await?.to_prepared_signature();

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
            .await?.to_prepared_signature();
        let sig_2 = authenticators[*signer2_idx]
            .get_signature(public_keys[*signer2_idx].into(), &tx_summary)
            .await?.to_prepared_signature();

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

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary).await?.to_prepared_signature();
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary).await?.to_prepared_signature();

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
