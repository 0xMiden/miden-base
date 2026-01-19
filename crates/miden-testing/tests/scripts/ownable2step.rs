extern crate alloc;

use alloc::sync::Arc;

use miden_processor::crypto::RpoRandomCoin;
use miden_protocol::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountIdVersion,
    AccountStorageMode,
    AccountType,
    StorageSlot,
    StorageSlotName,
};
use miden_protocol::assembly::DefaultSourceManager;
use miden_protocol::note::{NoteTag, NoteType};
use miden_protocol::transaction::OutputNote;
use miden_protocol::utils::sync::LazyLock;
use miden_protocol::{Felt, FieldElement, Word};
use miden_standards::code_builder::CodeBuilder;
use miden_standards::errors::standards::{
    ERR_NO_PENDING_OWNER,
    ERR_SENDER_NOT_OWNER,
    ERR_SENDER_NOT_PENDING_OWNER,
};
use miden_standards::testing::note::NoteBuilder;
use miden_testing::{Auth, MockChain, assert_transaction_executor_error};

static OWNERSHIP_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::access::ownable2step::ownership")
        .expect("storage slot name should be valid")
});

fn create_ownable2step_account(
    owner_account_id: AccountId,
    initial_storage: Vec<StorageSlot>,
) -> anyhow::Result<Account> {
    let component_code = r#"
        use miden::standards::access::ownable2step
        pub use ownable2step::get_owner
        pub use ownable2step::get_pending_owner
        pub use ownable2step::transfer_ownership
        pub use ownable2step::accept_ownership
        pub use ownable2step::cancel_transfer
        pub use ownable2step::renounce_ownership
    "#;
    let component_code_obj =
        CodeBuilder::default().compile_component_code("test::ownable2step", component_code)?;

    // get_item loads in REVERSE order: stack = [word[3], word[2], word[1], word[0]]
    // We want stack: [owner_prefix, owner_suffix, pending_prefix, pending_suffix]
    let ownership_word: Word = [
        Felt::ZERO,                                    // word[0] → stack[3] = pending_suffix
        Felt::ZERO,                                    // word[1] → stack[2] = pending_prefix
        Felt::new(owner_account_id.suffix().as_int()), // word[2] → stack[1] = owner_suffix
        owner_account_id.prefix().as_felt(),           // word[3] → stack[0] = owner_prefix
    ]
    .into();

    let mut storage_slots = initial_storage;
    storage_slots.push(StorageSlot::with_value(OWNERSHIP_SLOT_NAME.clone(), ownership_word));

    let account = AccountBuilder::new([1; 32])
        .storage_mode(AccountStorageMode::Public)
        .with_auth_component(Auth::IncrNonce)
        .with_component(
            AccountComponent::new(component_code_obj, storage_slots)?.with_supports_all_types(),
        )
        .build_existing()?;
    Ok(account)
}

fn get_owner_from_storage(account: &Account) -> anyhow::Result<(Felt, Felt)> {
    let word = account.storage().get_item(&OWNERSHIP_SLOT_NAME)?;
    Ok((word[3], word[2]))
}

fn get_pending_owner_from_storage(account: &Account) -> anyhow::Result<(Felt, Felt)> {
    let word = account.storage().get_item(&OWNERSHIP_SLOT_NAME)?;
    Ok((word[1], word[0]))
}

#[tokio::test]
async fn test_transfer_ownership_stores_pending() -> anyhow::Result<()> {
    let owner_account_id = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner_account_id = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner_account_id, vec![])?;
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner_account_id.suffix().as_int()),
        new_owner_account_id.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let note = NoteBuilder::new(owner_account_id, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(script.clone())
        .build()?;

    builder.add_output_note(OutputNote::Full(note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[note.id()], &[])?
        .add_note_script(note_script)
        .with_source_manager(source_manager)
        .build()?;
    let executed = tx.execute().await?;

    let mut updated = account.clone();
    updated.apply_delta(executed.account_delta())?;

    let (op, os) = get_owner_from_storage(&updated)?;
    assert_eq!(op, owner_account_id.prefix().as_felt());
    assert_eq!(os, Felt::new(owner_account_id.suffix().as_int()));

    let (pp, ps) = get_pending_owner_from_storage(&updated)?;
    assert_eq!(pp, new_owner_account_id.prefix().as_felt());
    assert_eq!(ps, Felt::new(new_owner_account_id.suffix().as_int()));
    Ok(())
}

#[tokio::test]
async fn test_transfer_ownership_only_owner() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let non_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [3; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner.suffix().as_int()),
        new_owner.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let note = NoteBuilder::new(non_owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(script)
        .build()?;

    builder.add_output_note(OutputNote::Full(note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[note.id()], &[])?
        .add_note_script(note_script)
        .with_source_manager(source_manager)
        .build()?;
    let result = tx.execute().await;

    assert_transaction_executor_error!(result, ERR_SENDER_NOT_OWNER);
    Ok(())
}

#[tokio::test]
async fn test_complete_ownership_transfer() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;

    // Step 1: transfer
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let transfer_script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner.suffix().as_int()),
        new_owner.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let transfer_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(transfer_script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let transfer_note = NoteBuilder::new(owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(transfer_script)
        .build()?;

    builder.add_output_note(OutputNote::Full(transfer_note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[transfer_note.id()], &[])?
        .add_note_script(transfer_note_script)
        .with_source_manager(source_manager.clone())
        .build()?;
    let executed = tx.execute().await?;

    let mut updated = account.clone();
    updated.apply_delta(executed.account_delta())?;

    // Step 2: accept
    let mut builder2 = MockChain::builder();
    builder2.add_account(updated.clone())?;

    let accept_script = r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.16 push.0 end
            call.test_account::accept_ownership
            dropw dropw dropw dropw
        end
    "#;

    let accept_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(accept_script)?;

    let mut rng2 = RpoRandomCoin::new([Felt::from(200u32); 4].into());
    let accept_note = NoteBuilder::new(new_owner, &mut rng2)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([50, 60, 70, 80u32]))
        .code(accept_script)
        .build()?;

    builder2.add_output_note(OutputNote::Full(accept_note.clone()));
    let mut mock_chain2 = builder2.build()?;
    mock_chain2.prove_next_block()?;

    let tx2 = mock_chain2
        .build_tx_context(updated.id(), &[accept_note.id()], &[])?
        .add_note_script(accept_note_script)
        .with_source_manager(source_manager)
        .build()?;
    let executed2 = tx2.execute().await?;

    let mut final_account = updated.clone();
    final_account.apply_delta(executed2.account_delta())?;

    let (op, os) = get_owner_from_storage(&final_account)?;
    assert_eq!(op, new_owner.prefix().as_felt());
    assert_eq!(os, Felt::new(new_owner.suffix().as_int()));

    let (pp, ps) = get_pending_owner_from_storage(&final_account)?;
    assert_eq!(pp, Felt::ZERO);
    assert_eq!(ps, Felt::ZERO);
    Ok(())
}

#[tokio::test]
async fn test_accept_ownership_only_pending_owner() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let wrong = AccountId::dummy(
        [3; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;

    // Step 1: transfer
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let transfer_script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner.suffix().as_int()),
        new_owner.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let transfer_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(transfer_script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let transfer_note = NoteBuilder::new(owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(transfer_script)
        .build()?;

    builder.add_output_note(OutputNote::Full(transfer_note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[transfer_note.id()], &[])?
        .add_note_script(transfer_note_script)
        .with_source_manager(source_manager.clone())
        .build()?;
    let executed = tx.execute().await?;

    let mut updated = account.clone();
    updated.apply_delta(executed.account_delta())?;

    // Step 2: wrong account tries accept
    let mut builder2 = MockChain::builder();
    builder2.add_account(updated.clone())?;

    let accept_script = r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.16 push.0 end
            call.test_account::accept_ownership
            dropw dropw dropw dropw
        end
    "#;

    let accept_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(accept_script)?;

    let mut rng2 = RpoRandomCoin::new([Felt::from(200u32); 4].into());
    let accept_note = NoteBuilder::new(wrong, &mut rng2)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([50, 60, 70, 80u32]))
        .code(accept_script)
        .build()?;

    builder2.add_output_note(OutputNote::Full(accept_note.clone()));
    let mut mock_chain2 = builder2.build()?;
    mock_chain2.prove_next_block()?;

    let tx2 = mock_chain2
        .build_tx_context(updated.id(), &[accept_note.id()], &[])?
        .add_note_script(accept_note_script)
        .with_source_manager(source_manager)
        .build()?;
    let result = tx2.execute().await;

    assert_transaction_executor_error!(result, ERR_SENDER_NOT_PENDING_OWNER);
    Ok(())
}

#[tokio::test]
async fn test_accept_ownership_no_pending() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let accept_script = r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.16 push.0 end
            call.test_account::accept_ownership
            dropw dropw dropw dropw
        end
    "#;

    let source_manager = Arc::new(DefaultSourceManager::default());
    let accept_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(accept_script)?;

    let mut rng = RpoRandomCoin::new([Felt::from(200u32); 4].into());
    let accept_note = NoteBuilder::new(new_owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([50, 60, 70, 80u32]))
        .code(accept_script)
        .build()?;

    builder.add_output_note(OutputNote::Full(accept_note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[accept_note.id()], &[])?
        .add_note_script(accept_note_script)
        .with_source_manager(source_manager)
        .build()?;
    let result = tx.execute().await;

    assert_transaction_executor_error!(result, ERR_NO_PENDING_OWNER);
    Ok(())
}

#[tokio::test]
async fn test_cancel_transfer() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;

    // Step 1: transfer
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let transfer_script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner.suffix().as_int()),
        new_owner.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let transfer_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(transfer_script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let transfer_note = NoteBuilder::new(owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(transfer_script)
        .build()?;

    builder.add_output_note(OutputNote::Full(transfer_note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[transfer_note.id()], &[])?
        .add_note_script(transfer_note_script)
        .with_source_manager(source_manager.clone())
        .build()?;
    let executed = tx.execute().await?;

    let mut updated = account.clone();
    updated.apply_delta(executed.account_delta())?;

    // Step 2: cancel
    let mut builder2 = MockChain::builder();
    builder2.add_account(updated.clone())?;

    let cancel_script = r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.16 push.0 end
            call.test_account::cancel_transfer
            dropw dropw dropw dropw
        end
    "#;

    let cancel_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(cancel_script)?;

    let mut rng2 = RpoRandomCoin::new([Felt::from(200u32); 4].into());
    let cancel_note = NoteBuilder::new(owner, &mut rng2)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([50, 60, 70, 80u32]))
        .code(cancel_script)
        .build()?;

    builder2.add_output_note(OutputNote::Full(cancel_note.clone()));
    let mut mock_chain2 = builder2.build()?;
    mock_chain2.prove_next_block()?;

    let tx2 = mock_chain2
        .build_tx_context(updated.id(), &[cancel_note.id()], &[])?
        .add_note_script(cancel_note_script)
        .with_source_manager(source_manager)
        .build()?;
    let executed2 = tx2.execute().await?;

    let mut final_account = updated.clone();
    final_account.apply_delta(executed2.account_delta())?;

    let (pp, ps) = get_pending_owner_from_storage(&final_account)?;
    assert_eq!(pp, Felt::ZERO);
    assert_eq!(ps, Felt::ZERO);

    let (op, os) = get_owner_from_storage(&final_account)?;
    assert_eq!(op, owner.prefix().as_felt());
    assert_eq!(os, Felt::new(owner.suffix().as_int()));
    Ok(())
}

#[tokio::test]
async fn test_renounce_ownership() -> anyhow::Result<()> {
    let owner = AccountId::dummy(
        [1; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );
    let new_owner = AccountId::dummy(
        [2; 15],
        AccountIdVersion::Version0,
        AccountType::RegularAccountImmutableCode,
        AccountStorageMode::Private,
    );

    let account = create_ownable2step_account(owner, vec![])?;

    // Step 1: transfer (to have pending)
    let mut builder = MockChain::builder();
    builder.add_account(account.clone())?;

    let transfer_script = format!(
        r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.14 push.0 end
            push.{} push.{}
            call.test_account::transfer_ownership
            dropw dropw dropw dropw
        end
    "#,
        Felt::new(new_owner.suffix().as_int()),
        new_owner.prefix().as_felt()
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let transfer_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(transfer_script.clone())?;

    let mut rng = RpoRandomCoin::new([Felt::from(100u32); 4].into());
    let transfer_note = NoteBuilder::new(owner, &mut rng)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([10, 20, 30, 40u32]))
        .code(transfer_script)
        .build()?;

    builder.add_output_note(OutputNote::Full(transfer_note.clone()));
    let mut mock_chain = builder.build()?;
    mock_chain.prove_next_block()?;

    let tx = mock_chain
        .build_tx_context(account.id(), &[transfer_note.id()], &[])?
        .add_note_script(transfer_note_script)
        .with_source_manager(source_manager.clone())
        .build()?;
    let executed = tx.execute().await?;

    let mut updated = account.clone();
    updated.apply_delta(executed.account_delta())?;

    // Step 2: renounce
    let mut builder2 = MockChain::builder();
    builder2.add_account(updated.clone())?;

    let renounce_script = r#"
        use miden::standards::access::ownable2step->test_account
        begin
            repeat.16 push.0 end
            call.test_account::renounce_ownership
            dropw dropw dropw dropw
        end
    "#;

    let renounce_note_script = CodeBuilder::with_source_manager(source_manager.clone())
        .compile_note_script(renounce_script)?;

    let mut rng2 = RpoRandomCoin::new([Felt::from(200u32); 4].into());
    let renounce_note = NoteBuilder::new(owner, &mut rng2)
        .note_type(NoteType::Private)
        .tag(NoteTag::default().into())
        .serial_number(Word::from([50, 60, 70, 80u32]))
        .code(renounce_script)
        .build()?;

    builder2.add_output_note(OutputNote::Full(renounce_note.clone()));
    let mut mock_chain2 = builder2.build()?;
    mock_chain2.prove_next_block()?;

    let tx2 = mock_chain2
        .build_tx_context(updated.id(), &[renounce_note.id()], &[])?
        .add_note_script(renounce_note_script)
        .with_source_manager(source_manager)
        .build()?;
    let executed2 = tx2.execute().await?;

    let mut final_account = updated.clone();
    final_account.apply_delta(executed2.account_delta())?;

    let (op, os) = get_owner_from_storage(&final_account)?;
    assert_eq!(op, Felt::ZERO);
    assert_eq!(os, Felt::ZERO);

    let (pp, ps) = get_pending_owner_from_storage(&final_account)?;
    assert_eq!(pp, Felt::ZERO);
    assert_eq!(ps, Felt::ZERO);
    Ok(())
}
