use miden_lib::{
    account::wallets::BasicWallet, note::create_p2id_note, transaction::TransactionKernel,
};
use miden_objects::{
    Felt, Hasher, Word,
    account::{AccountBuilder, AccountStorageMode, AccountType, AuthSecretKey},
    asset::FungibleAsset,
    crypto::dsa::rpo_falcon512::SecretKey,
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

#[test]
fn test_multisig() -> anyhow::Result<()> {
    // ROLES
    // - 2 Approvers (multisig signers)
    // - 1 Multisig Contract

    let mut rng = ChaCha20Rng::from_seed(Default::default());
    let sec_key = SecretKey::with_rng(&mut rng);
    let sec_key_2 = SecretKey::with_rng(&mut rng);
    let pub_key_1 = sec_key.public_key();
    let pub_key_2 = sec_key_2.public_key();

    println!("pubkey: {pub_key_1:?}");
    println!("pubkey: {pub_key_2:?}");

    let authenticator_1 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_1.into(), AuthSecretKey::RpoFalcon512(sec_key))],
        rng.clone(),
    );
    let authenticator_2 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_2.into(), AuthSecretKey::RpoFalcon512(sec_key_2))],
        rng,
    );

    let multisig_account = AccountBuilder::new([0; 32])
        .with_auth_component(Auth::Multisig {
            threshold: 2,
            approvers: vec![pub_key_1.into(), pub_key_2.into()],
        })
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_assets(vec![FungibleAsset::mock(10)])
        .build_existing()?;

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

    println!("Tx Summary gathered");

    // Get signatures from both approvers
    let msg = tx_summary.as_ref().to_commitment();
    let tx_summary = SigningInputs::TransactionSummary(tx_summary);

    let sig_1 = authenticator_1.get_signature(pub_key_1.into(), &tx_summary)?;
    let sig_2 = authenticator_2.get_signature(pub_key_2.into(), &tx_summary)?;

    // Populate advice map with signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[pub_key_1.into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[pub_key_2.into(), msg]), sig_2);

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
    let mut rng = ChaCha20Rng::from_seed([1u8; 32]);

    // Create 4 secret keys and public keys
    let sec_key_1 = SecretKey::with_rng(&mut rng);
    let sec_key_2 = SecretKey::with_rng(&mut rng);
    let sec_key_3 = SecretKey::with_rng(&mut rng);
    let sec_key_4 = SecretKey::with_rng(&mut rng);

    let pub_key_1 = sec_key_1.public_key();
    let pub_key_2 = sec_key_2.public_key();
    let pub_key_3 = sec_key_3.public_key();
    let pub_key_4 = sec_key_4.public_key();

    // Create authenticators for the first two signers only
    let authenticator_1 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_1.into(), AuthSecretKey::RpoFalcon512(sec_key_1))],
        rng.clone(),
    );
    let authenticator_2 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_2.into(), AuthSecretKey::RpoFalcon512(sec_key_2))],
        rng.clone(),
    );

    // Create multisig account with 4 approvers but threshold of 2
    let multisig_account = AccountBuilder::new([1; 32])
        .with_auth_component(Auth::Multisig {
            threshold: 2,
            approvers: vec![pub_key_1.into(), pub_key_2.into(), pub_key_3.into(), pub_key_4.into()],
        })
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_assets(vec![FungibleAsset::mock(10)])
        .build_existing()?;

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

    let sig_1 = authenticator_1.get_signature(pub_key_1.into(), &tx_summary)?;
    let sig_2 = authenticator_2.get_signature(pub_key_2.into(), &tx_summary)?;

    // Populate advice map with only 2 signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[pub_key_1.into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[pub_key_2.into(), msg]), sig_2);

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
fn test_multisig_replay_protection() -> anyhow::Result<()> {
    // Test 2/3 multisig where tx is executed, then attempted again (should fail on 2nd attempt)
    let mut rng = ChaCha20Rng::from_seed([2u8; 32]);

    // Create 3 secret keys and public keys
    let sec_key_1 = SecretKey::with_rng(&mut rng);
    let sec_key_2 = SecretKey::with_rng(&mut rng);
    let sec_key_3 = SecretKey::with_rng(&mut rng);

    let pub_key_1 = sec_key_1.public_key();
    let pub_key_2 = sec_key_2.public_key();
    let pub_key_3 = sec_key_3.public_key();

    // Create authenticators for 2 of the 3 signers
    let authenticator_1 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_1.into(), AuthSecretKey::RpoFalcon512(sec_key_1))],
        rng.clone(),
    );
    let authenticator_2 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_2.into(), AuthSecretKey::RpoFalcon512(sec_key_2))],
        rng.clone(),
    );

    // Create 2/3 multisig account
    let multisig_account = AccountBuilder::new([2; 32])
        .with_auth_component(Auth::Multisig {
            threshold: 2,
            approvers: vec![pub_key_1.into(), pub_key_2.into(), pub_key_3.into()],
        })
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_assets(vec![FungibleAsset::mock(20)])
        .build_existing()?;

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

    let sig_1 = authenticator_1.get_signature(pub_key_1.into(), &tx_summary)?;
    let sig_2 = authenticator_2.get_signature(pub_key_2.into(), &tx_summary)?;

    // Populate advice map with signatures
    let mut advice_map = AdviceMap::default();
    advice_map.insert(Hasher::merge(&[pub_key_1.into(), msg]), sig_1);
    advice_map.insert(Hasher::merge(&[pub_key_2.into(), msg]), sig_2);

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

    // This should fail - either due to insufficient funds or replay protection
    let result = tx_context_replay.execute();
    assert!(result.is_err(), "Second execution of the same transaction should fail");

    Ok(())
}
