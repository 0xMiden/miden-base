use assert_matches::assert_matches;
use miden_lib::{
    AuthScheme,
    account::wallets::{BasicWallet, create_basic_wallet},
    transaction::{TransactionKernel, TransactionKernelError},
};
use miden_objects::{
    Felt, FieldElement, Hasher, Word,
    account::{
        AccountBuilder, AccountComponent, AccountId, AccountStorage, AccountStorageMode,
        AccountType, AuthSecretKey,
    },
    asset::FungibleAsset,
    crypto::dsa::rpo_falcon512::{PublicKey, SecretKey},
    note::{NoteExecutionHint, NoteTag, NoteType},
    testing::{
        account_component::AccountMockComponent,
        account_id::{ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_SENDER},
        note::NoteBuilder,
    },
    transaction::{OutputNote, TransactionScript},
    utils::word_to_masm_push_string,
};
use miden_testing::{Auth, MockChain};
use miden_tx::{
    TransactionExecutorError,
    auth::{BasicAuthenticator, MultisigAuthenticator, TransactionAuthenticator},
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use vm_processor::ExecutionError;

#[test]
fn test_multisig() -> anyhow::Result<()> {
    let assembler = TransactionKernel::assembler();

    // ROLES
    // - 2 Approvers (`approver_1` is also the `Initiator`)
    // - 1 Coordinator
    // - 1 Multisig Contract

    let mut rng = ChaCha20Rng::from_seed(Default::default());
    let sec_key = SecretKey::with_rng(&mut rng);
    let sec_key_2 = SecretKey::with_rng(&mut rng);
    let pub_key_1 = sec_key.public_key();
    let pub_key_2 = sec_key_2.public_key();

    let authenticator_1 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_1.into(), AuthSecretKey::RpoFalcon512(sec_key))],
        rng,
    );
    let authenticator_2 = BasicAuthenticator::<ChaCha20Rng>::new_with_rng(
        &[(pub_key_2.into(), AuthSecretKey::RpoFalcon512(sec_key_2))],
        rng,
    );

    let (approver_1, _) = create_basic_wallet(
        [0; 32],
        AuthScheme::RpoFalcon512 { pub_key: pub_key_1 },
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Public,
    )?;

    let (approver_2, _) = create_basic_wallet(
        [0; 32],
        AuthScheme::RpoFalcon512 { pub_key: pub_key_2 },
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Public,
    )?;

    // TODO Add a new enum variant for this which under the hood creates a new component `AuthMultisigRpoFalcon512`. That component should store the number of approvers in one storage slot, the threshold in the other, and a map (index -> pubkey) in the third. Look at `RpoFalcon512ProcedureAcl` for how to store a map.
    let multisig_auth_component = Auth::Multisig {
        threshold: 2,
        approvers: vec![pub_key_1.into(), pub_key_2.into()],
    };

    let multisig_account = AccountBuilder::new([0; 32])
        .with_auth_component(multisig_auth_component)
        .with_component(BasicWallet)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    // In a non-mocked setting we would use the coordinator, here we just need to
    // create a special authenticator (done later on).
    let (_coordinator, _) = create_basic_wallet(
        [0; 32],
        AuthScheme::RpoFalcon512 { pub_key: PublicKey::new(Word::default()) },
        AccountType::RegularAccountUpdatableCode,
        AccountStorageMode::Public,
    )?;

    let mut mock_chain = MockChain::new();
    mock_chain.add_pending_account(multisig_account.clone());
    mock_chain.add_pending_account(approver_1.clone());
    mock_chain.add_pending_account(approver_2.clone());
    mock_chain.add_pending_account(_coordinator.clone());

    // Create a mock note to consume (needed to make the transaction non-empty)
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER)?;

    let note = NoteBuilder::new(sender_id, &mut rand::rng())
        .build(&assembler)
        .expect("failed to create mock note");

    mock_chain.add_pending_note(OutputNote::Full(note.clone()));
    mock_chain.prove_next_block()?;

    // INIT TX AND ADD FIRST SIGNATURE
    // -------------
    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET)?;
    let recipient = Word::from([0, 1, 2, 3u32]);
    let aux = Felt::new(27);
    let tag = NoteTag::from_account_id(faucet_id);
    let asset = Word::from(FungibleAsset::new(faucet_id, 10)?);

    // This script is executed in a special kernel that doesn't do auth (before epilogue)
    let tx_script_init_tx = format!(
        "
        use.miden::tx

        begin
            push.{recipient}
            push.{NOTE_EXECUTION_HINT}
            push.{PUBLIC_NOTE}
            push.{aux}
            push.{tag}

            call.tx::create_note
            # => [note_idx]

            push.{asset}
            call.tx::add_asset_to_note
            # => [ASSET, note_idx]

            dropw
            # => [note_idx]

            # truncate the stack
            swapdw dropw dropw
        end
        ",
        recipient = word_to_masm_push_string(&recipient),
        PUBLIC_NOTE = NoteType::Public as u8,
        NOTE_EXECUTION_HINT = Felt::from(NoteExecutionHint::always()),
        tag = tag,
        asset = word_to_masm_push_string(&asset),
    );

    let tx_script_send_note = TransactionScript::compile(
        tx_script_init_tx,
        TransactionKernel::testing_assembler_with_mock_account(),
    )?;

    let tx_context_init_tx = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[note.clone()])?
        .tx_script(tx_script_send_note.clone())
        .build()?;

    // Calling `execute_special` kernel that advances through the main tx script except for the epilogue,
    // Or otherwise getting the commitments and breaking out from execution.
    // This would normally by the coordinator doing this.
    let executed_tx_init_tx = tx_context_init_tx.execute()?;

    let error = tx_context_init_tx.execute().unwrap_err();

    let tx_effects = match error {
        TransactionExecutorError::AbortWithTxEffects(tx_effects) => tx_effects,
        _ => panic!("expected abort with tx effects"),
    };

    let aux = Word::default();

    let tx_hash = Hasher::hash(&[tx_effects, aux]);

    // TODO what do we do about the delta? Actually we should not be passing it here. The approvers should view `tx_effect`, which is more comprehensive than the delta.
    let sig_1 = authenticator_1.get_signature(pub_key_1, tx_hash)?;
    let sig_2 = authenticator_2.get_signature(pub_key_2, tx_hash)?;

    // for each public key, we add the vector of signatures that are associated with it. This is the authenticator that the coordinator uses.
    // TODO create a new authenticator MultisigAuthenticator
    // TODO fill the parameters correctly here.
    let mut multisig_authenticator = MultisigAuthenticator::new();
    multisig_authenticator.add_signature(pub_key_1.into(), tx_hash, sig_1);
    multisig_authenticator.add_signature(pub_key_2.into(), tx_hash, sig_2);
    // Approver_2 signs some other message
    let aux_2 = Word::from([1, 0, 0, 0]);
    let tx_hash_2 = Hasher::hash(&[tx_effects, aux_2]);
    let sig_2_b = authenticator_2.get_signature(pub_key_2, tx_hash_2)?;
    // We also add that signature to the coordinators' authenticator.
    // TODO the authenticator should have the capability to add more signatures for an existing or new public key
    multisig_authenticator.add_signature(pub_key_2.into(), tx_hash_2, sig_2_b);

    let tx_context_execute = mock_chain
        .build_tx_context(multisig_account.id(), &[], &[note.clone()])?
        .authenticator(Some(multisig_authenticator))
        .tx_script(tx_script_send_note)
        .build()?;

    tx_context_execute.execute().expect("adding signature should succeed");

    Ok(())
}
