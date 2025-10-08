use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use miden_lib::errors::tx_kernel_errors::{
    ERR_FOREIGN_ACCOUNT_CONTEXT_AGAINST_NATIVE_ACCOUNT,
    ERR_FOREIGN_ACCOUNT_INVALID_COMMITMENT,
    ERR_FOREIGN_ACCOUNT_MAX_NUMBER_EXCEEDED,
};
use miden_lib::testing::account_component::MockAccountComponent;
use miden_lib::transaction::TransactionKernel;
use miden_lib::transaction::memory::{
    ACCOUNT_DATA_LENGTH,
    ACCT_CODE_COMMITMENT_OFFSET,
    ACCT_ID_AND_NONCE_OFFSET,
    ACCT_PROCEDURES_SECTION_OFFSET,
    ACCT_STORAGE_COMMITMENT_OFFSET,
    ACCT_STORAGE_SLOTS_SECTION_OFFSET,
    ACCT_VAULT_ROOT_OFFSET,
    NATIVE_ACCOUNT_DATA_PTR,
    NUM_ACCT_PROCEDURES_OFFSET,
    NUM_ACCT_STORAGE_SLOTS_OFFSET,
};
use miden_lib::utils::ScriptBuilder;
use miden_objects::FieldElement;
use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountProcedureInfo,
    AccountStorage,
    AccountStorageMode,
    PartialAccount,
    StorageSlot,
};
use miden_objects::assembly::DefaultSourceManager;
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::asset::{Asset, FungibleAsset, NonFungibleAsset, NonFungibleAssetDetails};
use miden_objects::testing::account_id::{
    ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
    ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
};
use miden_objects::testing::storage::STORAGE_LEAVES_2;
use miden_objects::transaction::AccountInputs;
use miden_processor::{AdviceInputs, Felt};
use miden_tx::LocalTransactionProver;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use super::{Process, Word, ZERO};
use crate::kernel_tests::tx::ProcessMemoryExt;
use crate::{Auth, MockChainBuilder, assert_execution_error, assert_transaction_executor_error};

// SIMPLE FPI TESTS
// ================================================================================================

// FOREIGN PROCEDURE INVOCATION TESTS
// ================================================================================================

#[test]
fn test_fpi_memory_single_account() -> anyhow::Result<()> {
    // Prepare the test data
    let storage_slots =
        vec![AccountStorage::mock_item_0().slot, AccountStorage::mock_item_2().slot];
    let foreign_account_code_source = "
        use.miden::account

        export.get_item_foreign
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.1 drop
            exec.account::get_item

            # truncate the stack
            movup.6 movup.6 movup.6 drop drop drop
        end

        export.get_map_item_foreign
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.2 drop
            exec.account::get_map_item
        end
    ";

    let source_manager = Arc::new(DefaultSourceManager::default());
    let foreign_account_component = AccountComponent::compile(
        foreign_account_code_source,
        TransactionKernel::with_kernel_library(source_manager.clone()),
        storage_slots.clone(),
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component)
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_slots(vec![AccountStorage::mock_item_2().slot]))
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let fpi_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    let tx_context = mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(vec![fpi_inputs])
        .with_source_manager(source_manager)
        .build()?;

    // GET ITEM
    // --------------------------------------------------------------------------------------------
    // Check the correctness of the memory layout after `get_item_foreign` account procedure
    // invocation

    let code = format!(
        "
        use.std::sys

        use.$kernel::prologue
        use.miden::tx

        begin
            exec.prologue::prepare_transaction

            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign` procedure of the foreign account
            push.{get_item_foreign_hash}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(11)]

            exec.tx::execute_foreign_procedure
            # => [STORAGE_VALUE_1]

            # truncate the stack
            exec.sys::truncate_stack
            end
            ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
        get_item_foreign_hash = foreign_account.code().procedures()[1].mast_root(),
    );

    let process = tx_context.execute_code(&code)?;

    assert_eq!(
        process.stack.get_word(0),
        storage_slots[0].value(),
        "Value at the top of the stack (value in the storage at index 0) should be equal [1, 2, 3, 4]",
    );

    foreign_account_data_memory_assertions(&foreign_account, &process);

    // GET MAP ITEM
    // --------------------------------------------------------------------------------------------
    // Check the correctness of the memory layout after `get_map_item` account procedure invocation

    let code = format!(
        "
        use.std::sys

        use.$kernel::prologue
        use.miden::tx

        begin
            exec.prologue::prepare_transaction

            # pad the stack for the `execute_foreign_procedure` execution
            padw padw push.0.0
            # => [pad(10)]

            # push the key of desired storage item
            push.{map_key}

            # push the index of desired storage item
            push.1

            # get the hash of the `get_map_item_foreign` account procedure
            push.{get_map_item_foreign_hash}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, MAP_ITEM_KEY, pad(10)]

            exec.tx::execute_foreign_procedure
            # => [MAP_VALUE]

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
        map_key = STORAGE_LEAVES_2[0].0,
        get_map_item_foreign_hash = foreign_account.code().procedures()[2].mast_root(),
    );

    let process = tx_context.execute_code(&code).unwrap();

    assert_eq!(
        process.stack.get_word(0),
        STORAGE_LEAVES_2[0].1,
        "Value at the top of the stack should be equal [1, 2, 3, 4]",
    );

    foreign_account_data_memory_assertions(&foreign_account, &process);

    // GET ITEM TWICE
    // --------------------------------------------------------------------------------------------
    // Check the correctness of the memory layout after two consecutive invocations of the
    // `get_item` account procedures. Invoking two foreign procedures from the same account should
    // result in reuse of the loaded account.

    let code = format!(
        "
        use.std::sys

        use.$kernel::prologue
        use.miden::tx

        begin
            exec.prologue::prepare_transaction

            ### Get the storage item at index 0 #####################
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign` procedure of the foreign account
            push.{get_item_foreign_hash}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure dropw
            # => []

            ### Get the storage item at index 0 again ###############
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign` procedure of the foreign account
            push.{get_item_foreign_hash}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
        get_item_foreign_hash = foreign_account.code().procedures()[1].mast_root(),
    );

    let process = &tx_context.execute_code(&code)?;

    // Check that the second invocation of the foreign procedure from the same account does not load
    // the account data again: already loaded data should be reused.
    //
    // Native account:    [8192; 16383]  <- initialized during prologue
    // Foreign account:   [16384; 24575] <- initialized during first FPI
    // Next account slot: [24576; 32767] <- should not be initialized
    assert_eq!(
        process.try_get_kernel_mem_word(NATIVE_ACCOUNT_DATA_PTR + ACCOUNT_DATA_LENGTH as u32 * 2),
        None,
        "Memory starting from 24576 should stay uninitialized"
    );
    Ok(())
}

#[test]
fn test_fpi_memory_two_accounts() -> anyhow::Result<()> {
    // Prepare the test data
    let storage_slots_1 = vec![AccountStorage::mock_item_0().slot];
    let storage_slots_2 = vec![AccountStorage::mock_item_1().slot];

    let foreign_account_code_source_1 = "
        use.miden::account

        export.get_item_foreign_1
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.1 drop
            exec.account::get_item

            # truncate the stack
            movup.6 movup.6 movup.6 drop drop drop
        end
    ";
    let foreign_account_code_source_2 = "
        use.miden::account

        export.get_item_foreign_2
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.2 drop
            exec.account::get_item

            # truncate the stack
            movup.6 movup.6 movup.6 drop drop drop
        end
    ";

    let foreign_account_component_1 = AccountComponent::compile(
        foreign_account_code_source_1,
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        storage_slots_1.clone(),
    )?
    .with_supports_all_types();

    let foreign_account_component_2 = AccountComponent::compile(
        foreign_account_code_source_2,
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        storage_slots_2.clone(),
    )?
    .with_supports_all_types();

    let foreign_account_1 = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component_1)
        .build_existing()?;

    let foreign_account_2 = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component_2)
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain = MockChainBuilder::with_accounts([
        native_account.clone(),
        foreign_account_1.clone(),
        foreign_account_2.clone(),
    ])?
    .build()?;
    mock_chain.prove_next_block()?;
    let foreign_account_inputs_1 = mock_chain
        .get_foreign_account_inputs(foreign_account_1.id())
        .expect("failed to get foreign account inputs");

    let foreign_account_inputs_2 = mock_chain
        .get_foreign_account_inputs(foreign_account_2.id())
        .expect("failed to get foreign account inputs");

    let tx_context = mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(vec![foreign_account_inputs_1, foreign_account_inputs_2])
        .build()?;

    // GET ITEM TWICE WITH TWO ACCOUNTS
    // --------------------------------------------------------------------------------------------
    // Check the correctness of the memory layout after two invocations of the `get_item` account
    // procedures separated by the call of this procedure against another foreign account. Invoking
    // two foreign procedures from the same account should result in reuse of the loaded account.

    let code = format!(
        "
        use.std::sys

        use.$kernel::prologue
        use.miden::tx

        begin
            exec.prologue::prepare_transaction

            ### Get the storage item at index 0 from the first account
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign_1` procedure of the foreign account 1
            push.{get_item_foreign_1_hash}

            # push the foreign account ID
            push.{foreign_1_suffix} push.{foreign_1_prefix}
            # => [foreign_account_1_id_prefix, foreign_account_1_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure dropw
            # => []

            ### Get the storage item at index 0 from the second account
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign_2` procedure of the foreign account 2
            push.{get_item_foreign_2_hash}

            # push the foreign account ID
            push.{foreign_2_suffix} push.{foreign_2_prefix}
            # => [foreign_account_2_id_prefix, foreign_account_2_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure dropw
            # => []

            ### Get the storage item at index 0 from the first account again
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign_1` procedure of the foreign account 1
            push.{get_item_foreign_1_hash}

            # push the foreign account ID
            push.{foreign_1_suffix} push.{foreign_1_prefix}
            # => [foreign_account_1_id_prefix, foreign_account_1_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        get_item_foreign_1_hash = foreign_account_1.code().procedures()[1].mast_root(),
        get_item_foreign_2_hash = foreign_account_2.code().procedures()[1].mast_root(),

        foreign_1_prefix = foreign_account_1.id().prefix().as_felt(),
        foreign_1_suffix = foreign_account_1.id().suffix(),

        foreign_2_prefix = foreign_account_2.id().prefix().as_felt(),
        foreign_2_suffix = foreign_account_2.id().suffix(),
    );

    let process = &tx_context.execute_code(&code)?;

    // Check the correctness of the memory layout after multiple foreign procedure invocations from
    // different foreign accounts
    //
    // Native account:    [8192; 16383]  <- initialized during prologue
    // Foreign account 1: [16384; 24575] <- initialized during first FPI
    // Foreign account 2: [24576; 32767] <- initialized during second FPI
    // Next account slot: [32768; 40959] <- should not be initialized

    // check that the first word of the first foreign account slot is correct
    assert_eq!(
        process.get_kernel_mem_word(NATIVE_ACCOUNT_DATA_PTR + ACCOUNT_DATA_LENGTH as u32),
        Word::new([
            foreign_account_1.id().suffix(),
            foreign_account_1.id().prefix().as_felt(),
            ZERO,
            foreign_account_1.nonce()
        ])
    );

    // check that the first word of the second foreign account slot is correct
    assert_eq!(
        process.get_kernel_mem_word(NATIVE_ACCOUNT_DATA_PTR + ACCOUNT_DATA_LENGTH as u32 * 2),
        Word::new([
            foreign_account_2.id().suffix(),
            foreign_account_2.id().prefix().as_felt(),
            ZERO,
            foreign_account_2.nonce()
        ])
    );

    // check that the first word of the third foreign account slot was not initialized
    assert_eq!(
        process.try_get_kernel_mem_word(NATIVE_ACCOUNT_DATA_PTR + ACCOUNT_DATA_LENGTH as u32 * 3),
        None,
        "Memory starting from 32768 should stay uninitialized"
    );

    Ok(())
}

/// Test the correctness of the foreign procedure execution.
///
/// It checks the foreign account code loading, providing the mast forest to the executor,
/// construction of the account procedure maps and execution the foreign procedure in order to
/// obtain the data from the foreign account's storage slot.
#[tokio::test]
async fn test_fpi_execute_foreign_procedure() -> anyhow::Result<()> {
    // Prepare the test data
    let storage_slots =
        vec![AccountStorage::mock_item_0().slot, AccountStorage::mock_item_2().slot];
    let foreign_account_code_source = "
        use.miden::account

        export.get_item_foreign
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.1 drop
            exec.account::get_item

            # truncate the stack
            movup.6 movup.6 movup.6 drop drop drop
        end

        export.get_map_item_foreign
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.2 drop
            exec.account::get_map_item
        end
    ";

    let source_manager = Arc::new(DefaultSourceManager::default());
    let foreign_account_component = AccountComponent::compile(
        NamedSource::new("foreign_account", foreign_account_code_source),
        TransactionKernel::with_kernel_library(source_manager.clone()),
        storage_slots,
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component.clone())
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        "
        use.std::sys

        use.miden::tx
        use.miden::account

        begin
            # get the storage item at index 0
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.0

            # get the hash of the `get_item_foreign` account procedure
            procref.::foreign_account::get_item_foreign

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure
            # => [STORAGE_VALUE]

            # assert the correctness of the obtained value
            push.1.2.3.4 assert_eqw
            # => []

            # get the storage map at index 1
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw push.0.0
            # => [pad(10)]

            # push the key of desired storage item
            push.{map_key}

            # push the index of desired storage item
            push.1

            # get the hash of the `get_map_item_foreign` account procedure
            procref.::foreign_account::get_map_item_foreign

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, MAP_ITEM_KEY, pad(10)]

            exec.tx::execute_foreign_procedure
            # => [MAP_VALUE]

            # assert the correctness of the obtained value
            push.1.2.3.4 assert_eqw
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
        map_key = STORAGE_LEAVES_2[0].0,
    );

    let tx_script = ScriptBuilder::with_source_manager(source_manager.clone())
        .with_dynamically_linked_library(foreign_account_component.library())?
        .compile_tx_script(code)?;

    let foreign_account_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts([foreign_account_inputs])
        .enable_lazy_loading()
        .tx_script(tx_script)
        .with_source_manager(source_manager)
        .build()?
        .execute()
        .await?;

    Ok(())
}

/// Test that a foreign account can get the balance of a fungible asset and check the presence of a
/// non-fungible asset.
#[tokio::test]
async fn foreign_account_can_get_balance_and_presence_of_asset() -> anyhow::Result<()> {
    let fungible_faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1)?;
    let non_fungible_faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET)?;

    // Create two different assets.
    let fungible_asset = Asset::Fungible(FungibleAsset::new(fungible_faucet_id, 1)?);
    let non_fungible_asset = Asset::NonFungible(NonFungibleAsset::new(
        &NonFungibleAssetDetails::new(non_fungible_faucet_id.prefix(), vec![1, 2, 3])?,
    )?);

    let foreign_account_code_source = format!(
        "
        use.miden::account

        export.get_asset_balance
            # get balance of first asset
            push.{fungible_faucet_id_suffix} push.{fungible_faucet_id_prefix}
            exec.account::get_balance
            # => [balance]

            # check presence of non fungible asset
            push.{non_fungible_asset_word}
            exec.account::has_non_fungible_asset
            # => [has_asset, balance]

            # add the balance and the bool
            add
            # => [has_asset_balance]

            # keep only the result on stack
            swap drop
            # => [has_asset_balance]
        end
        ",
        fungible_faucet_id_prefix = fungible_faucet_id.prefix().as_felt(),
        fungible_faucet_id_suffix = fungible_faucet_id.suffix(),
        non_fungible_asset_word = Word::from(non_fungible_asset),
    );

    let source_manager = Arc::new(DefaultSourceManager::default());
    let foreign_account_component = AccountComponent::compile(
        NamedSource::new("foreign_account_code", foreign_account_code_source),
        TransactionKernel::with_kernel_library(source_manager.clone()),
        vec![],
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component.clone())
        .with_assets(vec![fungible_asset, non_fungible_asset])
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        "
        use.std::sys

        use.miden::tx
        use.miden::account

        begin
            # Get the added balance of two assets from foreign account
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `get_asset_balance` procedure
            procref.::foreign_account_code::get_asset_balance

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, pad(15)]

            exec.tx::execute_foreign_procedure
            # => [has_asset_balance]

            # assert that the non fungible asset exists and the fungible asset has balance 1
            push.2 assert_eq.err=\"Total balance should be 2\"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
    );

    let tx_script = ScriptBuilder::with_source_manager(source_manager.clone())
        .with_dynamically_linked_library(foreign_account_component.library())?
        .compile_tx_script(code)?;

    let foreign_account_inputs = mock_chain.get_foreign_account_inputs(foreign_account.id())?;

    mock_chain
        .build_tx_context(native_account.id(), &[], &[])?
        .foreign_accounts([foreign_account_inputs])
        .enable_lazy_loading()
        .tx_script(tx_script)
        .with_source_manager(source_manager)
        .build()?
        .execute()
        .await?;

    Ok(())
}

// NESTED FPI TESTS
// ================================================================================================

/// Test the correctness of the cyclic foreign procedure calls.
///
/// It checks that the account data pointers are correctly added and removed from the account data
/// stack.
///
/// The call chain in this test looks like so:
/// `Native -> First FA -> Second FA -> First FA`
#[tokio::test]
async fn test_nested_fpi_cyclic_invocation() -> anyhow::Result<()> {
    // ------ SECOND FOREIGN ACCOUNT ---------------------------------------------------------------
    let storage_slots = vec![AccountStorage::mock_item_0().slot];
    let second_foreign_account_code_source = r#"
        use.miden::tx
        use.miden::account

        use.std::sys

        export.second_account_foreign_proc
            # get the storage item at index 1
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0
            # => [pad(14)]

            # push the index of desired storage item
            push.1

            # get the hash of the `get_item_foreign` account procedure from the advice stack
            adv_push.4

            # push the foreign account ID from the advice stack
            adv_push.2
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure
            # => [storage_value]

            # make sure that the resulting value equals 5
            dup push.5 assert_eq.err="value should have been 5"

            # get the first element of the 0'th storage slot (it should be 1) and add it to the
            # obtained foreign value.
            push.0 exec.account::get_item drop drop drop
            add

            # assert that the resulting value equals 6
            dup push.6 assert_eq.err="value should have been 6"

            exec.sys::truncate_stack
        end
    "#;

    let source_manager = Arc::new(DefaultSourceManager::default());
    let second_foreign_account_component = AccountComponent::compile(
        second_foreign_account_code_source,
        TransactionKernel::with_kernel_library(source_manager.clone()),
        storage_slots,
    )?
    .with_supports_all_types();

    let second_foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(second_foreign_account_component)
        .build_existing()?;

    // ------ FIRST FOREIGN ACCOUNT ---------------------------------------------------------------
    let storage_slots =
        vec![AccountStorage::mock_item_0().slot, AccountStorage::mock_item_1().slot];
    let first_foreign_account_code_source = r#"
        use.miden::tx
        use.miden::account

        use.std::sys

        export.first_account_foreign_proc
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `second_account_foreign_proc` account procedure from the advice stack
            adv_push.4

            # push the ID of the second foreign account from the advice stack
            adv_push.2
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure
            # => [storage_value]

            # get the second element of the 0'th storage slot (it should be 2) and add it to the
            # obtained foreign value.
            push.0 exec.account::get_item drop drop swap drop
            add

            # assert that the resulting value equals 8
            dup push.8 assert_eq.err="value should have been 8"

            exec.sys::truncate_stack
        end

        export.get_item_foreign
            # make this foreign procedure unique to make sure that we invoke the procedure of the
            # foreign account, not the native one
            push.1 drop
            exec.account::get_item

            # return the first element of the resulting word
            drop drop drop
        end
    "#;

    let first_foreign_account_component = AccountComponent::compile(
        NamedSource::new("first_foreign_account", first_foreign_account_code_source),
        TransactionKernel::with_kernel_library(source_manager.clone()),
        storage_slots,
    )?
    .with_supports_all_types();

    let first_foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(first_foreign_account_component.clone())
        .build_existing()?;

    // ------ NATIVE ACCOUNT ---------------------------------------------------------------
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain = MockChainBuilder::with_accounts([
        native_account.clone(),
        first_foreign_account.clone(),
        second_foreign_account.clone(),
    ])?
    .build()?;
    mock_chain.prove_next_block()?;
    let foreign_account_inputs = vec![
        mock_chain
            .get_foreign_account_inputs(first_foreign_account.id())
            .expect("failed to get foreign account inputs"),
        mock_chain
            .get_foreign_account_inputs(second_foreign_account.id())
            .expect("failed to get foreign account inputs"),
    ];

    // push the hashes of the foreign procedures and account IDs to the advice stack to be able to
    // call them dynamically.
    let mut advice_inputs = AdviceInputs::default();
    advice_inputs
        .stack
        .extend(*second_foreign_account.code().procedures()[1].mast_root());
    advice_inputs.stack.extend([
        second_foreign_account.id().suffix(),
        second_foreign_account.id().prefix().as_felt(),
    ]);

    advice_inputs
        .stack
        .extend(*first_foreign_account.code().procedures()[2].mast_root());
    advice_inputs.stack.extend([
        first_foreign_account.id().suffix(),
        first_foreign_account.id().prefix().as_felt(),
    ]);

    let code = format!(
        r#"
        use.std::sys

        use.miden::tx
        use.miden::account

        begin
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `first_account_foreign_proc` procedure
            procref.::first_foreign_account::first_account_foreign_proc

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure
            # => [storage_value]

            # add 10 to the returning value
            add.10

            # assert that the resulting value equals 18
            push.18 assert_eq.err="sum should be 18"
            # => []

            exec.sys::truncate_stack
        end
        "#,
        foreign_prefix = first_foreign_account.id().prefix().as_felt(),
        foreign_suffix = first_foreign_account.id().suffix(),
    );

    let tx_script = ScriptBuilder::with_source_manager(source_manager.clone())
        .with_dynamically_linked_library(first_foreign_account_component.library())?
        .compile_tx_script(code)?;

    let executed_transaction = mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(foreign_account_inputs)
        .enable_lazy_loading()
        .extend_advice_inputs(advice_inputs)
        .tx_script(tx_script)
        .with_source_manager(source_manager)
        .build()?
        .execute()
        .await?;

    // TODO: Remove later and add a integration test using FPI.
    LocalTransactionProver::default().prove(executed_transaction.into())?;

    Ok(())
}

/// Test that code will panic in attempt to create more than 63 foreign accounts.
///
/// Attempt to create a 64th foreign account first triggers the assert during the account data
/// loading, but we have an additional assert during the account stack push just in case.
#[tokio::test]
async fn test_nested_fpi_stack_overflow() -> anyhow::Result<()> {
    let mut foreign_accounts = Vec::new();

    let last_foreign_account_code_source = "
                use.miden::account

                export.get_item_foreign
                    # make this foreign procedure unique to make sure that we invoke the procedure
                    # of the foreign account, not the native one
                    push.1 drop

                    # push the index of desired storage item
                    push.0

                    exec.account::get_item

                    # return the first element of the resulting word
                    drop drop drop

                    # make sure that the resulting value equals 1
                    assert
                end
        ";

    let storage_slots = vec![AccountStorage::mock_item_0().slot];
    let last_foreign_account_component = AccountComponent::compile(
        last_foreign_account_code_source,
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        storage_slots,
    )
    .unwrap()
    .with_supports_all_types();

    let last_foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(last_foreign_account_component)
        .build_existing()
        .unwrap();

    foreign_accounts.push(last_foreign_account);

    for foreign_account_index in 0..63 {
        let next_account = foreign_accounts.last().unwrap();

        let foreign_account_code_source = format!(
                    "
                use.miden::tx
                use.std::sys

                export.read_first_foreign_storage_slot_{foreign_account_index}
                    # pad the stack for the `execute_foreign_procedure` execution
                    padw padw padw push.0.0.0
                    # => [pad(15)]

                    # get the hash of the `get_item` account procedure
                    push.{next_account_proc_hash}

                    # push the foreign account ID
                    push.{next_foreign_suffix} push.{next_foreign_prefix}
                    # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

                    exec.tx::execute_foreign_procedure
                    # => [storage_value]

                    exec.sys::truncate_stack
                end
            ",
                    next_account_proc_hash = next_account.code().procedures()[1].mast_root(),
                    next_foreign_suffix = next_account.id().suffix(),
                    next_foreign_prefix = next_account.id().prefix().as_felt(),
                );

        let foreign_account_component = AccountComponent::compile(
            foreign_account_code_source,
            TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
            vec![],
        )
        .unwrap()
        .with_supports_all_types();

        let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
            .with_auth_component(Auth::IncrNonce)
            .with_component(foreign_account_component)
            .build_existing()
            .unwrap();

        foreign_accounts.push(foreign_account)
    }

    // ------ NATIVE ACCOUNT ---------------------------------------------------------------
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()
        .unwrap();

    let mut mock_chain = MockChainBuilder::with_accounts(
        [vec![native_account.clone()], foreign_accounts.clone()].concat(),
    )
    .unwrap()
    .build()
    .unwrap();

    mock_chain.prove_next_block().unwrap();

    let foreign_accounts: Vec<AccountInputs> = foreign_accounts
        .iter()
        .map(|acc| {
            mock_chain
                .get_foreign_account_inputs(acc.id())
                .expect("failed to get foreign account inputs")
        })
        .collect();

    let code = format!(
                "
            use.std::sys

            use.miden::tx

            begin
                # pad the stack for the `execute_foreign_procedure` execution
                padw padw padw push.0.0.0
                # => [pad(15)]

                # get the hash of the `get_item` account procedure
                push.{foreign_account_proc_hash}

                # push the foreign account ID
                push.{foreign_suffix} push.{foreign_prefix}
                # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

                exec.tx::execute_foreign_procedure
                # => [storage_value]

                exec.sys::truncate_stack
            end
            ",
                foreign_account_proc_hash =
                    foreign_accounts.last().unwrap().code().procedures()[1].mast_root(),
                foreign_prefix = foreign_accounts.last().unwrap().id().prefix().as_felt(),
                foreign_suffix = foreign_accounts.last().unwrap().id().suffix(),
            );

    let tx_script = ScriptBuilder::default().compile_tx_script(code).unwrap();

    let tx_context = mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(foreign_accounts)
        .enable_lazy_loading()
        .tx_script(tx_script)
        .build()
        .unwrap();

    let result = tx_context.execute().await;

    assert_transaction_executor_error!(result, ERR_FOREIGN_ACCOUNT_MAX_NUMBER_EXCEEDED);
    Ok(())
}

/// Test that code will panic in attempt to call a procedure from the native account.
#[tokio::test]
async fn test_nested_fpi_native_account_invocation() -> anyhow::Result<()> {
    // ------ FIRST FOREIGN ACCOUNT ---------------------------------------------------------------
    let foreign_account_code_source = "
        use.miden::tx

        use.std::sys

        export.first_account_foreign_proc
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the native account procedure from the advice stack
            adv_push.4

            # push the ID of the native account from the advice stack
            adv_push.2
            # => [native_account_id_prefix, native_account_id_suffix, NATIVE_PROC_ROOT, pad(15)]

            exec.tx::execute_foreign_procedure
            # => [storage_value]

            exec.sys::truncate_stack
        end
    ";

    let foreign_account_component = AccountComponent::compile(
        NamedSource::new("foreign_account", foreign_account_code_source),
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        vec![],
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component.clone())
        .build_existing()?;

    // ------ NATIVE ACCOUNT ---------------------------------------------------------------
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block().unwrap();

    let code = format!(
        "
        use.std::sys

        use.miden::tx

        begin
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `get_item` account procedure
            push.{first_account_foreign_proc_hash}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, storage_item_index, pad(14)]

            exec.tx::execute_foreign_procedure
            # => [storage_value]

            exec.sys::truncate_stack
        end
        ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
        first_account_foreign_proc_hash = foreign_account.code().procedures()[1].mast_root(),
    );

    let tx_script = ScriptBuilder::default()
        .with_dynamically_linked_library(foreign_account_component.library())?
        .compile_tx_script(code)?;

    let foreign_account_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    // push the hash of the native procedure and native account IDs to the advice stack to be able
    // to call them dynamically.
    let mut advice_inputs = AdviceInputs::default();
    advice_inputs.stack.extend(*native_account.code().procedures()[3].mast_root());
    advice_inputs
        .stack
        .extend([native_account.id().suffix(), native_account.id().prefix().as_felt()]);

    let result = mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(vec![foreign_account_inputs])
        .enable_lazy_loading()
        .extend_advice_inputs(advice_inputs)
        .tx_script(tx_script)
        .build()?
        .execute()
        .await;

    assert_transaction_executor_error!(result, ERR_FOREIGN_ACCOUNT_CONTEXT_AGAINST_NATIVE_ACCOUNT);
    Ok(())
}

/// Test that providing an account whose commitment does not match the one in the account tree
/// results in an error.
#[tokio::test]
async fn test_fpi_stale_account() -> anyhow::Result<()> {
    // Prepare the test data
    let foreign_account_code_source = "
        use.miden::account

        # code is not used in this test
        export.set_some_item_foreign
            push.34.1
            exec.account::set_item
        end
    ";

    let foreign_account_component = AccountComponent::compile(
        foreign_account_code_source,
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        vec![AccountStorage::mock_item_0().slot],
    )?
    .with_supports_all_types();

    let mut foreign_account = AccountBuilder::new([5; 32])
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component)
        .build_existing()?;

    let native_account = AccountBuilder::new([4; 32])
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_slots(vec![AccountStorage::mock_item_2().slot]))
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    // Make the foreign account invalid.
    // --------------------------------------------------------------------------------------------

    // Modify the account's storage to change its storage commitment and in turn the account
    // commitment.
    foreign_account
        .storage_mut()
        .set_item(0, Word::from([Felt::ONE, Felt::ONE, Felt::ONE, Felt::ONE]))?;

    // Place the modified account in the advice provider, which will cause the commitment mismatch.
    let foreign_account_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    // We want to create a mixed ForeignAccountInputs because we want to have a valid account
    // witness against the ref block, but have newer account data (ie, a new state). Otherwise,
    // any non-validity of the account witness is caught in
    // TransactionExecutor::execute_transaction() (see `test_fpi_anchoring_validations()` for
    // context on this check)
    let overridden_partial_accounts = PartialAccount::new(
        foreign_account.id(),
        foreign_account.nonce(),
        foreign_account.code().clone(),
        foreign_account.storage().into(),
        foreign_account.vault().into(),
        None,
    )?;
    let overridden_foreign_account_inputs =
        AccountInputs::new(overridden_partial_accounts, foreign_account_inputs.witness().clone());

    // The account tree from which the transaction inputs are fetched here has the state from the
    // original unmodified foreign account. This should result in the foreign account's proof to be
    // invalid for this account tree root.
    let tx_context = mock_chain
        .build_tx_context(native_account, &[], &[])?
        .foreign_accounts(vec![overridden_foreign_account_inputs])
        .build()?;

    // Attempt to run FPI.
    // --------------------------------------------------------------------------------------------

    let code = format!(
        "
      use.std::sys

      use.$kernel::prologue
      use.miden::tx

      begin
          exec.prologue::prepare_transaction

          # pad the stack for the `execute_foreign_procedure` execution
          padw padw padw padw
          # => [pad(16)]

          # push some hash onto the stack - for this test it does not matter
          padw
          # => [FOREIGN_PROC_ROOT, pad(16)]

          # push the foreign account ID
          push.{foreign_suffix} push.{foreign_prefix}
          # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, pad(16)]

          exec.tx::execute_foreign_procedure
        end
      ",
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        foreign_suffix = foreign_account.id().suffix(),
    );

    let result = tx_context.execute_code(&code).map(|_| ());
    assert_execution_error!(result, ERR_FOREIGN_ACCOUNT_INVALID_COMMITMENT);
    Ok(())
}

/// This test checks that our `miden::get_id` and `miden::get_native_id` procedures return IDs of
/// the current and native account respectively while being called from the foreign account.
#[tokio::test]
async fn test_fpi_get_account_id() -> anyhow::Result<()> {
    let foreign_account_code_source = "
        use.miden::account

        export.get_current_and_native_ids
            # get the ID of the current (foreign) account
            exec.account::get_id
            # => [acct_id_prefix, acct_id_suffix, pad(16)]

            # get the ID of the native account
            exec.account::get_native_id
            # => [native_acct_id_prefix, native_acct_id_suffix, acct_id_prefix, acct_id_suffix, pad(16)]

            # truncate the stack
            swapw dropw
            # => [native_acct_id_prefix, native_acct_id_suffix, acct_id_prefix, acct_id_suffix, pad(12)]
        end
    ";

    let foreign_account_component = AccountComponent::compile(
        NamedSource::new("foreign_account", foreign_account_code_source),
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        Vec::new(),
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component.clone())
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        r#"
        use.std::sys

        use.miden::tx
        use.miden::account
        use.miden::account_id

        begin
            # get the IDs of the foreign and native accounts
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `get_current_and_native_ids` foreign account procedure
            procref.::foreign_account::get_current_and_native_ids

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, pad(15)]

            exec.tx::execute_foreign_procedure
            # => [native_acct_id_prefix, native_acct_id_suffix, acct_id_prefix, acct_id_suffix]

            # push the expected native account ID and check that it is equal to the one returned
            # from the FPI
            push.{expected_native_suffix} push.{expected_native_prefix}
            exec.account_id::is_equal
            assert.err="native account ID returned from the FPI is not equal to the expected one"
            # => [acct_id_prefix, acct_id_suffix]

            # push the expected foreign account ID and check that it is equal to the one returned
            # from the FPI
            push.{foreign_suffix} push.{foreign_prefix}
            exec.account_id::is_equal
            assert.err="foreign account ID returned from the FPI is not equal to the expected one"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        foreign_suffix = foreign_account.id().suffix(),
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        expected_native_suffix = native_account.id().suffix(),
        expected_native_prefix = native_account.id().prefix().as_felt(),
    );

    let tx_script = ScriptBuilder::default()
        .with_dynamically_linked_library(foreign_account_component.library())?
        .compile_tx_script(code)?;

    let foreign_account_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(vec![foreign_account_inputs])
        .enable_lazy_loading()
        .tx_script(tx_script)
        .build()?
        .execute()
        .await?;

    Ok(())
}

/// This test checks that our `miden::get_nonce` and `miden::get_native_nonce` procedures return
/// nonce values of the current and native account respectively while being called from the foreign
/// account.
#[tokio::test]
async fn test_fpi_get_account_nonce() -> anyhow::Result<()> {
    let foreign_account_code_source = "
        use.miden::account

        export.get_current_and_native_nonce_values
            # get the nonce of the current (foreign) account
            exec.account::get_nonce
            # => [nonce, pad(16)]

            # get the nonce of the native account
            exec.account::get_native_nonce
            # => [native_nonce, nonce, pad(16)]

            # truncate the stack
            movdn.3 movdn.3 drop drop
            # => [native_nonce, nonce, pad(14)]
        end
    ";

    let foreign_account_component = AccountComponent::compile(
        foreign_account_code_source,
        TransactionKernel::with_kernel_library(Arc::new(DefaultSourceManager::default())),
        Vec::new(),
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component)
        .build_existing()?;

    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .nonce(Felt::new(2))
        .build_existing()?;

    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let code = format!(
        r#"
        use.std::sys

        use.miden::tx
        use.miden::account

        begin
            # get the nonce values of the foreign and native accounts
            # pad the stack for the `execute_foreign_procedure` execution
            padw padw padw push.0.0.0
            # => [pad(15)]

            # get the hash of the `get_current_and_native_nonce_values` foreign account procedure
            push.{get_current_and_native_nonce_values}

            # push the foreign account ID
            push.{foreign_suffix} push.{foreign_prefix}
            # => [foreign_account_id_prefix, foreign_account_id_suffix, FOREIGN_PROC_ROOT, pad(15)]

            exec.tx::execute_foreign_procedure
            # => [native_nonce, foreign_nonce]

            # push the expected native account nonce and check that it is equal to the one returned
            # from the FPI
            push.{expected_native_nonce}
            assert_eq.err="native account nonce returned from the FPI is not equal to the expected one"
            # => [foreign_nonce]

            # push the expected foreign account nonce and check that it is equal to the one returned
            # from the FPI
            push.{expected_foreign_nonce}
            assert_eq.err="foreign account nonce returned from the FPI is not equal to the expected one"
            # => []

            # truncate the stack
            exec.sys::truncate_stack
        end
        "#,
        get_current_and_native_nonce_values = foreign_account.code().procedures()[1].mast_root(),
        foreign_suffix = foreign_account.id().suffix(),
        foreign_prefix = foreign_account.id().prefix().as_felt(),
        expected_native_nonce = native_account.nonce(),
        expected_foreign_nonce = foreign_account.nonce(),
    );

    let tx_script = ScriptBuilder::default().compile_tx_script(code)?;

    let foreign_account_inputs = mock_chain
        .get_foreign_account_inputs(foreign_account.id())
        .expect("failed to get foreign account inputs");

    mock_chain
        .build_tx_context(native_account.id(), &[], &[])
        .expect("failed to build tx context")
        .foreign_accounts(vec![foreign_account_inputs])
        .enable_lazy_loading()
        .tx_script(tx_script)
        .build()?
        .execute()
        .await?;

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

fn foreign_account_data_memory_assertions(foreign_account: &Account, process: &Process) {
    let foreign_account_data_ptr = NATIVE_ACCOUNT_DATA_PTR + ACCOUNT_DATA_LENGTH as u32;

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + ACCT_ID_AND_NONCE_OFFSET),
        Word::new([
            foreign_account.id().suffix(),
            foreign_account.id().prefix().as_felt(),
            ZERO,
            foreign_account.nonce()
        ]),
    );

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + ACCT_VAULT_ROOT_OFFSET),
        foreign_account.vault().root(),
    );

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + ACCT_STORAGE_COMMITMENT_OFFSET),
        foreign_account.storage().commitment(),
    );

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + ACCT_CODE_COMMITMENT_OFFSET),
        foreign_account.code().commitment(),
    );

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + NUM_ACCT_STORAGE_SLOTS_OFFSET),
        Word::from([u16::try_from(foreign_account.storage().slots().len()).unwrap(), 0, 0, 0]),
    );

    for (i, elements) in foreign_account
        .storage()
        .as_elements()
        .chunks(StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT / 2)
        .enumerate()
    {
        assert_eq!(
            process.get_kernel_mem_word(
                foreign_account_data_ptr + ACCT_STORAGE_SLOTS_SECTION_OFFSET + (i as u32) * 4
            ),
            Word::try_from(elements).unwrap(),
        )
    }

    assert_eq!(
        process.get_kernel_mem_word(foreign_account_data_ptr + NUM_ACCT_PROCEDURES_OFFSET),
        Word::from([u16::try_from(foreign_account.code().num_procedures()).unwrap(), 0, 0, 0]),
    );

    for (i, elements) in foreign_account
        .code()
        .as_elements()
        .chunks(AccountProcedureInfo::NUM_ELEMENTS_PER_PROC / 2)
        .enumerate()
    {
        assert_eq!(
            process.get_kernel_mem_word(
                foreign_account_data_ptr + ACCT_PROCEDURES_SECTION_OFFSET + (i as u32) * 4
            ),
            Word::try_from(elements).unwrap(),
        );
    }
}

/// Test that get_initial_item and get_initial_map_item work correctly with foreign accounts.
#[tokio::test]
async fn test_get_initial_item_and_get_initial_map_item_with_foreign_account() -> anyhow::Result<()>
{
    // Create a native account
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(MockAccountComponent::with_empty_slots())
        .storage_mode(AccountStorageMode::Public)
        .build_existing()?;

    let (map_key, map_value) = STORAGE_LEAVES_2[0];

    // Create foreign procedures that test get_initial_item and get_initial_map_item
    let foreign_account_code_source = "
        use.miden::account
        use.std::sys

        export.test_get_initial_item
            push.0
            exec.account::get_initial_item
            exec.sys::truncate_stack
        end

        export.test_get_initial_map_item
            exec.account::get_initial_map_item
            exec.sys::truncate_stack
        end
    ";

    let foreign_account_component = AccountComponent::compile(
        NamedSource::new("foreign_account", foreign_account_code_source),
        TransactionKernel::assembler(),
        vec![AccountStorage::mock_item_0().slot, AccountStorage::mock_item_2().slot],
    )?
    .with_supports_all_types();

    let foreign_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(foreign_account_component.clone())
        .build_existing()?;

    // Create the mock chain with both accounts
    let mut mock_chain =
        MockChainBuilder::with_accounts([native_account.clone(), foreign_account.clone()])?
            .build()?;
    mock_chain.prove_next_block()?;

    let foreign_account_inputs = mock_chain.get_foreign_account_inputs(foreign_account.id())?;

    let code = format!(
        "
        use.std::sys
        use.miden::tx

        begin

            # Test get_initial_item on foreign account
            padw padw padw push.0.0.0
            # => [ pad(4), pad(4), pad(4), 0, 0, 0 ]
            procref.::foreign_account::test_get_initial_item
            push.{foreign_account_id_suffix} push.{foreign_account_id_prefix}
            exec.tx::execute_foreign_procedure
            push.{expected_value_slot_0}
            assert_eqw.err=\"foreign account get_initial_item should work\"

            # Test get_initial_map_item on foreign account
            padw padw push.0.0
            push.{map_key}
            push.1
            procref.::foreign_account::test_get_initial_map_item
            push.{foreign_account_id_suffix} push.{foreign_account_id_prefix}
            exec.tx::execute_foreign_procedure
            push.{map_value}
            assert_eqw.err=\"foreign account get_initial_map_item should work\"

            exec.sys::truncate_stack
        end
        ",
        foreign_account_id_prefix = foreign_account.id().prefix().as_felt(),
        foreign_account_id_suffix = foreign_account.id().suffix(),
        expected_value_slot_0 = &AccountStorage::mock_item_0().slot.value(),
        map_key = &map_key,
        map_value = &map_value,
    );

    let tx_script = ScriptBuilder::with_mock_libraries()?
        .with_dynamically_linked_library(foreign_account_component.library())?
        .compile_tx_script(code)?;

    mock_chain
        .build_tx_context(native_account.id(), &[], &[])?
        .foreign_accounts(vec![foreign_account_inputs])
        .tx_script(tx_script)
        .build()?
        .execute()
        .await?;

    Ok(())
}
