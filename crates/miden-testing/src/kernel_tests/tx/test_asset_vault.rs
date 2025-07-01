use assert_matches::assert_matches;
use miden_lib::{
    errors::tx_kernel_errors::{
        ERR_VAULT_FUNGIBLE_ASSET_AMOUNT_LESS_THAN_AMOUNT_TO_WITHDRAW,
        ERR_VAULT_FUNGIBLE_MAX_AMOUNT_EXCEEDED,
        ERR_VAULT_GET_BALANCE_PROC_CAN_ONLY_BE_CALLED_ON_FUNGIBLE_FAUCET,
        ERR_VAULT_NON_FUNGIBLE_ASSET_ALREADY_EXISTS,
        ERR_VAULT_NON_FUNGIBLE_ASSET_TO_REMOVE_NOT_FOUND,
    },
    transaction::{TransactionKernel, memory},
    utils::word_to_masm_push_string,
};
use miden_objects::{
    AssetVaultError,
    account::AccountId,
    asset::{Asset, FungibleAsset, NonFungibleAsset, NonFungibleAssetDetails},
    testing::{
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
            ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1,
        },
        constants::{FUNGIBLE_ASSET_AMOUNT, NON_FUNGIBLE_ASSET_DATA},
    },
};
use vm_processor::ProcessState;

use super::{Felt, ONE, Word, ZERO};
use crate::{
    TransactionContextBuilder, assert_execution_error, kernel_tests::tx::read_root_mem_word,
};

#[test]
fn test_get_balance() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();

    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let code = format!(
        "
        use.kernel::prologue
        use.miden::account

        begin
            exec.prologue::prepare_transaction
            push.{suffix}.{prefix}
            exec.account::get_balance

            # truncate the stack
            swap drop
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();

    assert_eq!(
        process.stack.get(0).as_int(),
        tx_context.account().vault().get_balance(faucet_id).unwrap()
    );
}

#[test]
fn test_get_balance_non_fungible_fails() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET).unwrap();
    let code = format!(
        "
        use.kernel::prologue
        use.miden::account

        begin
            exec.prologue::prepare_transaction
            push.{suffix}.{prefix}
            exec.account::get_balance
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(
        process,
        ERR_VAULT_GET_BALANCE_PROC_CAN_ONLY_BE_CALLED_ON_FUNGIBLE_FAUCET
    );
}

#[test]
fn test_has_non_fungible_asset() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let non_fungible_asset =
        tx_context.account().vault().assets().find(Asset::is_non_fungible).unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.miden::account

        begin
            exec.prologue::prepare_transaction
            push.{non_fungible_asset_key}
            exec.account::has_non_fungible_asset

            # truncate the stack
            swap drop
        end
        ",
        non_fungible_asset_key = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();

    assert_eq!(process.stack.get(0), ONE);
}

#[test]
fn test_add_fungible_asset_success() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let mut account_vault = tx_context.account().vault().clone();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let amount = FungibleAsset::MAX_AMOUNT - FUNGIBLE_ASSET_AMOUNT;
    let add_fungible_asset = Asset::try_from([
        Felt::new(amount),
        ZERO,
        faucet_id.suffix(),
        faucet_id.prefix().as_felt(),
    ])
    .unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::add_asset

            # truncate the stack
            swapw dropw
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&add_fungible_asset.into())
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    assert_eq!(
        process_state.get_stack_word(0),
        Into::<Word>::into(account_vault.add_asset(add_fungible_asset).unwrap())
    );

    assert_eq!(
        read_root_mem_word(&process_state, memory::NATIVE_ACCT_VAULT_ROOT_PTR),
        *account_vault.root()
    );
}

#[test]
fn test_add_non_fungible_asset_fail_overflow() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let mut account_vault = tx_context.account().vault().clone();

    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let amount = FungibleAsset::MAX_AMOUNT - FUNGIBLE_ASSET_AMOUNT + 1;
    let add_fungible_asset = Asset::try_from([
        Felt::new(amount),
        ZERO,
        faucet_id.suffix(),
        faucet_id.prefix().as_felt(),
    ])
    .unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::add_asset
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&add_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_VAULT_FUNGIBLE_MAX_AMOUNT_EXCEEDED);
    assert!(account_vault.add_asset(add_fungible_asset).is_err());
}

#[test]
fn test_add_non_fungible_asset_success() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET.try_into().unwrap();
    let mut account_vault = tx_context.account().vault().clone();
    let add_non_fungible_asset = Asset::NonFungible(
        NonFungibleAsset::new(
            &NonFungibleAssetDetails::new(faucet_id.prefix(), vec![1, 2, 3, 4, 5, 6, 7, 8])
                .unwrap(),
        )
        .unwrap(),
    );

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::add_asset

            # truncate the stack
            swapw dropw
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&add_non_fungible_asset.into())
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    assert_eq!(
        process_state.get_stack_word(0),
        Into::<Word>::into(account_vault.add_asset(add_non_fungible_asset).unwrap())
    );

    assert_eq!(
        read_root_mem_word(&process_state, memory::NATIVE_ACCT_VAULT_ROOT_PTR),
        *account_vault.root()
    );
}

#[test]
fn test_add_non_fungible_asset_fail_duplicate() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET.try_into().unwrap();
    let mut account_vault = tx_context.account().vault().clone();
    let non_fungible_asset_details =
        NonFungibleAssetDetails::new(faucet_id.prefix(), NON_FUNGIBLE_ASSET_DATA.to_vec()).unwrap();
    let non_fungible_asset =
        Asset::NonFungible(NonFungibleAsset::new(&non_fungible_asset_details).unwrap());

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{NON_FUNGIBLE_ASSET}
            call.account::add_asset
        end
        ",
        NON_FUNGIBLE_ASSET = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_VAULT_NON_FUNGIBLE_ASSET_ALREADY_EXISTS);
    assert!(account_vault.add_asset(non_fungible_asset).is_err());
}

#[test]
fn test_remove_fungible_asset_success_no_balance_remaining() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let mut account_vault = tx_context.account().vault().clone();

    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let amount = FUNGIBLE_ASSET_AMOUNT;
    let remove_fungible_asset = Asset::try_from([
        Felt::new(amount),
        ZERO,
        faucet_id.suffix(),
        faucet_id.prefix().as_felt(),
    ])
    .unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::remove_asset

            # truncate the stack
            swapw dropw
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&remove_fungible_asset.into())
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    assert_eq!(
        process_state.get_stack_word(0),
        Into::<Word>::into(account_vault.remove_asset(remove_fungible_asset).unwrap())
    );

    assert_eq!(
        read_root_mem_word(&process_state, memory::NATIVE_ACCT_VAULT_ROOT_PTR),
        *account_vault.root()
    );
}

#[test]
fn test_remove_fungible_asset_fail_remove_too_much() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let amount = FUNGIBLE_ASSET_AMOUNT + 1;
    let remove_fungible_asset = Asset::try_from([
        Felt::new(amount),
        ZERO,
        faucet_id.suffix(),
        faucet_id.prefix().as_felt(),
    ])
    .unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::remove_asset
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&remove_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_VAULT_FUNGIBLE_ASSET_AMOUNT_LESS_THAN_AMOUNT_TO_WITHDRAW);
}

#[test]
fn test_remove_fungible_asset_success_balance_remaining() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let mut account_vault = tx_context.account().vault().clone();

    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
    let amount = FUNGIBLE_ASSET_AMOUNT - 1;
    let remove_fungible_asset = Asset::try_from([
        Felt::new(amount),
        ZERO,
        faucet_id.suffix(),
        faucet_id.prefix().as_felt(),
    ])
    .unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::remove_asset

            # truncate the stack
            swapw dropw
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&remove_fungible_asset.into())
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    assert_eq!(
        process_state.get_stack_word(0),
        Into::<Word>::into(account_vault.remove_asset(remove_fungible_asset).unwrap())
    );

    assert_eq!(
        read_root_mem_word(&process_state, memory::NATIVE_ACCT_VAULT_ROOT_PTR),
        *account_vault.root()
    );
}

#[test]
fn test_remove_inexisting_non_fungible_asset_fails() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1.try_into().unwrap();
    let mut account_vault = tx_context.account().vault().clone();

    let non_fungible_asset_details =
        NonFungibleAssetDetails::new(faucet_id.prefix(), NON_FUNGIBLE_ASSET_DATA.to_vec()).unwrap();
    let nonfungible = NonFungibleAsset::new(&non_fungible_asset_details).unwrap();
    let non_existent_non_fungible_asset = Asset::NonFungible(nonfungible);

    assert_matches!(
        account_vault.remove_asset(non_existent_non_fungible_asset).unwrap_err(),
        AssetVaultError::NonFungibleAssetNotFound(err_asset) if err_asset == nonfungible,
        "asset must not be in the vault before the test",
    );

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::remove_asset
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&non_existent_non_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_VAULT_NON_FUNGIBLE_ASSET_TO_REMOVE_NOT_FOUND);
    assert_matches!(
        account_vault.remove_asset(non_existent_non_fungible_asset).unwrap_err(),
        AssetVaultError::NonFungibleAssetNotFound(err_asset) if err_asset == nonfungible,
        "asset should not be in the vault after the test",
    );
}

#[test]
fn test_remove_non_fungible_asset_success() {
    let tx_context = TransactionContextBuilder::with_existing_standard_account().build();
    let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET.try_into().unwrap();
    let mut account_vault = tx_context.account().vault().clone();
    let non_fungible_asset_details =
        NonFungibleAssetDetails::new(faucet_id.prefix(), NON_FUNGIBLE_ASSET_DATA.to_vec()).unwrap();
    let non_fungible_asset =
        Asset::NonFungible(NonFungibleAsset::new(&non_fungible_asset_details).unwrap());

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET}
            call.account::remove_asset

            # truncate the stack
            swapw dropw
        end
        ",
        FUNGIBLE_ASSET = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    assert_eq!(
        process_state.get_stack_word(0),
        Into::<Word>::into(account_vault.remove_asset(non_fungible_asset).unwrap())
    );

    assert_eq!(
        read_root_mem_word(&process_state, memory::NATIVE_ACCT_VAULT_ROOT_PTR),
        *account_vault.root()
    );
}
