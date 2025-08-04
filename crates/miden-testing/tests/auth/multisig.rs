use miden_lib::{
    account::wallets::BasicWallet, note::create_p2id_note, transaction::TransactionKernel,
};
use miden_objects::{
    Felt, Hasher, Word,
    account::{Account, AccountBuilder, AccountStorageMode, AccountType, AuthSecretKey},
    asset::FungibleAsset,
    crypto::dsa::rpo_falcon512::{PublicKey, SecretKey},
    note::NoteType,
    testing::account_id::{
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
    },
    transaction::{OutputNote, TransactionScript},
    vm::AdviceMap,
};
use miden_testing::{Auth, MockChainBuilder};
use miden_tx::{
    TransactionExecutorError,
    auth::{BasicAuthenticator, SigningInputs, TransactionAuthenticator},
    utils::word_to_masm_push_string,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use vm_processor::crypto::RpoRandomCoin;

// ================================================================================================
// HELPER FUNCTIONS
// ================================================================================================

type MultisigTestSetup = (Vec<SecretKey>, Vec<PublicKey>, Vec<BasicAuthenticator<ChaCha20Rng>>);

/// Sets up secret keys, public keys, and authenticators for multisig testing
fn setup_keys_and_authenticators(
    num_approvers: usize,
    num_signers: usize, // How many of the approvers will actually sign
    seed: [u8; 32],
) -> anyhow::Result<MultisigTestSetup> {
    let mut rng = ChaCha20Rng::from_seed(seed);

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
    for i in 0..num_signers {
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

#[test]
fn test_multisig() -> anyhow::Result<()> {
    // ROLES
    // - 2 Approvers (multisig signers)
    // - 1 Multisig Contract

    // Setup keys and authenticators
    let (_secret_keys, public_keys, authenticators) =
        setup_keys_and_authenticators(2, 2, [0u8; 32])?;

    // Create multisig account
    let multisig_account = create_multisig_account(2, &public_keys, 10)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    mock_chain.prove_next_block()?;

    // Create output note for the transaction
    let output_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([3u32; 4])),
    )?;

    let asset = FungibleAsset::mock(10);
    let tx_script = TransactionScript::compile(
        format!(
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
            recipient = word_to_masm_push_string(&output_note.recipient().digest()),
            note_type = NoteType::Public as u8,
            tag = Felt::from(output_note.metadata().tag()),
            asset = word_to_masm_push_string(&asset.into()),
            note_execution_hint = Felt::from(output_note.metadata().execution_hint()),
        ),
        TransactionKernel::testing_assembler(),
    )?;

    let salt = Word::from([Felt::new(1); 4]);

    // Execute transaction without signatures - should fail
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script.clone())
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .auth_args(salt)
        .build()?;

    let tx_summary = match tx_context_init.execute().unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from both approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary)?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary)?;

    // Populate advice map with signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[public_keys[0].into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[public_keys[1].into(), msg]), sig_2);

    // Execute transaction with signatures - should succeed
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .extend_advice_map(advice_map)
        .auth_args(salt)
        .build()?;
    tx_context_execute.execute().expect("Transaction should succeed");

    Ok(())
}

#[test]
fn test_multisig_4_owners_threshold_2() -> anyhow::Result<()> {
    // Test 4 owners with threshold 2 - only 2 signatures should be needed

    // Setup keys and authenticators (4 approvers, but only 2 signers)
    let (_secret_keys, public_keys, authenticators) =
        setup_keys_and_authenticators(4, 2, [1u8; 32])?;

    // Create multisig account with 4 approvers but threshold of 2
    let multisig_account = create_multisig_account(2, &public_keys, 10)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    mock_chain.prove_next_block()?;

    // Create output note for the transaction
    let output_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([4u32; 4])),
    )?;

    let asset = FungibleAsset::mock(5);
    let tx_script = TransactionScript::compile(
        format!(
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
            recipient = word_to_masm_push_string(&output_note.recipient().digest()),
            note_type = NoteType::Public as u8,
            tag = Felt::from(output_note.metadata().tag()),
            asset = word_to_masm_push_string(&asset.into()),
            note_execution_hint = Felt::from(output_note.metadata().execution_hint()),
        ),
        TransactionKernel::testing_assembler(),
    )?;

    let salt = Word::from([Felt::new(2); 4]);

    // Execute transaction without signatures - should fail
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script.clone())
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .auth_args(salt)
        .build()?;

    let tx_summary = match tx_context_init.execute().unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from only 2 of the 4 approvers (should be sufficient for threshold 2)
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary)?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary)?;

    // Populate advice map with only 2 signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[public_keys[0].into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[public_keys[1].into(), msg]), sig_2);

    // Execute transaction with only 2 signatures - should succeed since threshold is 2
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .extend_advice_map(advice_map)
        .auth_args(salt)
        .build()?;

    tx_context_execute
        .execute()
        .expect("Transaction should succeed with 2 out of 4 signatures");

    Ok(())
}

#[test]
fn test_multisig_4_owners_threshold_2_different_signer_combinations() -> anyhow::Result<()> {
    // Test 4 owners with threshold 2 - test different combinations of signers
    // This tests that any 2 of the 4 approvers can sign, not just the first 2

    // Setup keys and authenticators (4 approvers, all 4 can sign)
    let (_secret_keys, public_keys, authenticators) =
        setup_keys_and_authenticators(4, 4, [3u8; 32])?;

    // Create multisig account with 4 approvers but threshold of 2
    let multisig_account = create_multisig_account(2, &public_keys, 10)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    mock_chain.prove_next_block()?;

    // Test different combinations of 2 signers out of 4
    let signer_combinations = vec![
        (0, 1), // First two
        (0, 2), // First and third
        (0, 3), // First and fourth
        (1, 2), // Second and third
        (1, 3), // Second and fourth
        (2, 3), // Last two
    ];

    for (i, (signer1_idx, signer2_idx)) in signer_combinations.iter().enumerate() {
        // Create output note for the transaction
        let output_note = create_p2id_note(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
            vec![],
            NoteType::Public,
            Default::default(),
            &mut RpoRandomCoin::new(Word::from([10u32 + i as u32; 4])),
        )?;

        let asset = FungibleAsset::mock(1);
        let tx_script = TransactionScript::compile(
            format!(
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
                recipient = word_to_masm_push_string(&output_note.recipient().digest()),
                note_type = NoteType::Public as u8,
                tag = Felt::from(output_note.metadata().tag()),
                asset = word_to_masm_push_string(&asset.into()),
                note_execution_hint = Felt::from(output_note.metadata().execution_hint()),
            ),
            TransactionKernel::testing_assembler(),
        )?;

        let salt = Word::from([Felt::new(10 + i as u64); 4]);

        // Execute transaction without signatures first to get tx summary
        let tx_context_init = mock_chain
            .build_tx_context(multisig_account.id(), &[], &[])?
            .tx_script(tx_script.clone())
            .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
            .auth_args(salt)
            .build()?;

        let tx_summary = match tx_context_init.execute().unwrap_err() {
            TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
            error => panic!("expected abort with tx effects: {error:?}"),
        };

        // Get signatures from the specific combination of signers
        let msg = tx_summary.as_ref().to_commitment();
        let tx_summary = SigningInputs::TransactionSummary(tx_summary);

        let sig_1 = authenticators[*signer1_idx]
            .get_signature(public_keys[*signer1_idx].into(), &tx_summary)?;
        let sig_2 = authenticators[*signer2_idx]
            .get_signature(public_keys[*signer2_idx].into(), &tx_summary)?;

        // Populate advice map with signatures from the chosen signers
        let mut advice_map = AdviceMap::default();
        advice_map.insert(Hasher::merge(&[public_keys[*signer1_idx].into(), msg]), sig_1);
        advice_map.insert(Hasher::merge(&[public_keys[*signer2_idx].into(), msg]), sig_2);

        // Execute transaction with signatures - should succeed for any combination
        let tx_context_execute = mock_chain
            .build_tx_context(multisig_account.id(), &[], &[])?
            .tx_script(tx_script)
            .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
            .extend_advice_map(advice_map)
            .auth_args(salt)
            .build()?;

        let executed_tx = tx_context_execute.execute().expect(&format!(
            "Transaction should succeed with signers {} and {}",
            signer1_idx, signer2_idx
        ));

        // Apply the transaction to the mock chain for the next iteration
        mock_chain.add_pending_executed_transaction(&executed_tx)?;
        mock_chain.prove_next_block()?;
    }

    Ok(())
}

#[test]
fn test_multisig_replay_protection() -> anyhow::Result<()> {
    // Test 2/3 multisig where tx is executed, then attempted again (should fail on 2nd attempt)

    // Setup keys and authenticators (3 approvers, but only 2 signers)
    let (_secret_keys, public_keys, authenticators) =
        setup_keys_and_authenticators(3, 2, [2u8; 32])?;

    // Create 2/3 multisig account
    let multisig_account = create_multisig_account(2, &public_keys, 20)?;

    let mut mock_chain = MockChainBuilder::with_accounts([multisig_account.clone()])
        .unwrap()
        .build()
        .unwrap();

    mock_chain.prove_next_block()?;

    // Create output note for the transaction
    let output_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([5u32; 4])),
    )?;

    let asset = FungibleAsset::mock(3);
    let tx_script = TransactionScript::compile(
        format!(
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
            recipient = word_to_masm_push_string(&output_note.recipient().digest()),
            note_type = NoteType::Public as u8,
            tag = Felt::from(output_note.metadata().tag()),
            asset = word_to_masm_push_string(&asset.into()),
            note_execution_hint = Felt::from(output_note.metadata().execution_hint()),
        ),
        TransactionKernel::testing_assembler(),
    )?;

    let salt = Word::from([Felt::new(3); 4]);

    // Execute transaction without signatures first to get tx summary
    let tx_context_init = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script.clone())
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .auth_args(salt)
        .build()?;

    let tx_summary = match tx_context_init.execute().unwrap_err() {
        TransactionExecutorError::Unauthorized(tx_effects) => tx_effects,
        error => panic!("expected abort with tx effects: {error:?}"),
    };

    // Get signatures from 2 of the 3 approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticators[0].get_signature(public_keys[0].into(), &tx_summary)?;
    let sig_2 = authenticators[1].get_signature(public_keys[1].into(), &tx_summary)?;

    // Populate advice map with signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[public_keys[0].into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[public_keys[1].into(), msg]), sig_2);

    // Execute transaction with signatures - should succeed (first execution)
    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script.clone())
        .extend_expected_output_notes(vec![OutputNote::Full(output_note.clone())])
        .extend_advice_map(advice_map.clone())
        .auth_args(salt)
        .build()?;

    let executed_tx = tx_context_execute.execute().expect("First transaction should succeed");

    // Apply the transaction to the mock chain
    mock_chain.add_pending_executed_transaction(&executed_tx)?;
    mock_chain.prove_next_block()?;

    // Now attempt to execute the same transaction again - should fail due to replay protection
    let tx_context_replay = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[])?
        .tx_script(tx_script)
        .extend_expected_output_notes(vec![OutputNote::Full(output_note)])
        .extend_advice_map(advice_map)
        .auth_args(salt)
        .build()?;

    // This should fail - due to replay protection
    let result = tx_context_replay.execute();
    assert!(result.is_err(), "Second execution of the same transaction should fail");

    Ok(())
}
