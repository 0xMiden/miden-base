use miden_lib::{
    errors::tx_kernel_errors::{
        ERR_FAUCET_BURN_NON_FUNGIBLE_ASSET_CAN_ONLY_BE_CALLED_ON_NON_FUNGIBLE_FAUCET,
        ERR_FAUCET_NEW_TOTAL_SUPPLY_WOULD_EXCEED_MAX_ASSET_AMOUNT,
        ERR_FAUCET_NON_FUNGIBLE_ASSET_ALREADY_ISSUED,
        ERR_FAUCET_NON_FUNGIBLE_ASSET_TO_BURN_NOT_FOUND, ERR_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN,
        ERR_NON_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN,
        ERR_VAULT_FUNGIBLE_ASSET_AMOUNT_LESS_THAN_AMOUNT_TO_WITHDRAW,
    },
    transaction::{TransactionKernel, memory::NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR},
    utils::word_to_masm_push_string,
};
use miden_objects::{
    FieldElement,
    account::{AccountId, StorageMap},
    asset::{FungibleAsset, NonFungibleAsset},
    testing::{
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
            ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1,
        },
        constants::{
            CONSUMED_ASSET_1_AMOUNT, FUNGIBLE_ASSET_AMOUNT, FUNGIBLE_FAUCET_INITIAL_BALANCE,
            NON_FUNGIBLE_ASSET_DATA, NON_FUNGIBLE_ASSET_DATA_2,
        },
        storage::FAUCET_STORAGE_DATA_SLOT,
    },
};
use vm_processor::{Felt, ONE, ProcessState};

use crate::{TransactionContextBuilder, assert_execution_error};

// FUNGIBLE FAUCET MINT TESTS
// ================================================================================================

#[test]
fn test_mint_fungible_asset_succeeds() {
    let tx_context = TransactionContextBuilder::with_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ONE,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
    )
    .build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();

    let code = format!(
        r#"
        use.test::account
        use.kernel::asset_vault
        use.kernel::memory
        use.kernel::prologue

        begin
            # mint asset
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::mint

            # assert the correct asset is returned
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            assert_eqw.err="minted asset does not match expected asset"

            # assert the input vault has been updated
            exec.memory::get_input_vault_root_ptr
            push.{suffix}.{prefix}
            exec.asset_vault::get_balance
            push.{FUNGIBLE_ASSET_AMOUNT} assert_eq.err="input vault should contain minted asset"
        end
        "#,
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    let expected_final_storage_amount = FUNGIBLE_FAUCET_INITIAL_BALANCE + FUNGIBLE_ASSET_AMOUNT;
    let faucet_reserved_slot_storage_location =
        FAUCET_STORAGE_DATA_SLOT as u32 + NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR;
    let faucet_storage_amount_location = faucet_reserved_slot_storage_location + 3;

    let faucet_storage_amount = process_state
        .get_mem_value(process_state.ctx(), faucet_storage_amount_location)
        .unwrap()
        .as_int();

    assert_eq!(faucet_storage_amount, expected_final_storage_amount);
}

#[test]
fn test_mint_fungible_asset_fails_not_faucet_account() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::mint
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_mint_fungible_asset_inconsistent_faucet_id() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();
    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::mint
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_mint_fungible_asset_fails_saturate_max_amount() {
    let tx_context = TransactionContextBuilder::with_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        Felt::ONE,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
    )
    .build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{saturating_amount}.0.{suffix}.{prefix}
            call.account::mint
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
        saturating_amount = FungibleAsset::MAX_AMOUNT - FUNGIBLE_FAUCET_INITIAL_BALANCE + 1
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FAUCET_NEW_TOTAL_SUPPLY_WOULD_EXCEED_MAX_ASSET_AMOUNT);
}

// NON-FUNGIBLE FAUCET MINT TESTS
// ================================================================================================

#[test]
fn test_mint_non_fungible_asset_succeeds() {
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        NonFungibleAsset::mock_issuer().into(),
        ONE,
        false,
    )
    .build();

    let non_fungible_asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let asset_vault_key = non_fungible_asset.vault_key();

    let code = format!(
        r#"
        use.std::collections::smt

        use.kernel::account
        use.kernel::asset_vault
        use.kernel::memory
        use.kernel::prologue
        use.test::account->test_account

        begin
            # mint asset
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.test_account::mint

            # assert the correct asset is returned
            push.{non_fungible_asset}
            assert_eqw.err="minted asset does not match expected asset"

            # assert the input vault has been updated.
            exec.memory::get_input_vault_root_ptr
            push.{non_fungible_asset}
            exec.asset_vault::has_non_fungible_asset
            assert.err="vault should contain asset"

            # assert the non-fungible asset has been added to the faucet smt
            push.{FAUCET_STORAGE_DATA_SLOT}
            exec.account::get_item
            push.{asset_vault_key}
            exec.smt::get
            push.{non_fungible_asset}
            assert_eqw.err="minted asset should have been added to faucet SMT"
            dropw
        end
        "#,
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset.into()),
        asset_vault_key = word_to_masm_push_string(&StorageMap::hash_key(asset_vault_key.into())),
    );

    tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
}

#[test]
fn test_mint_non_fungible_asset_fails_not_faucet_account() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let non_fungible_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::mint
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_NON_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_mint_non_fungible_asset_fails_inconsistent_faucet_id() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let non_fungible_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::mint
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_NON_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_mint_non_fungible_asset_fails_asset_already_exists() {
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        NonFungibleAsset::mock_issuer().into(),
        ONE,
        false,
    )
    .build();

    let non_fungible_asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::mint
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FAUCET_NON_FUNGIBLE_ASSET_ALREADY_ISSUED);
}

// FUNGIBLE FAUCET BURN TESTS
// ================================================================================================

#[test]
fn test_burn_fungible_asset_succeeds() {
    let tx_context = TransactionContextBuilder::with_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
        ONE,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
    )
    .with_mock_notes_preserved()
    .build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();

    let code = format!(
        r#"
        use.test::account
        use.kernel::asset_vault
        use.kernel::memory
        use.kernel::prologue

        begin
            # burn asset
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::burn

            # assert the correct asset is returned
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            assert_eqw.err="burnt asset does not match expected asset"

            # assert the input vault has been updated
            exec.memory::get_input_vault_root_ptr

            push.{suffix}.{prefix}
            exec.asset_vault::get_balance
            
            push.{final_input_vault_asset_amount} assert_eq.err="vault balance does not match expected balance"
        end
        "#,
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
        final_input_vault_asset_amount = CONSUMED_ASSET_1_AMOUNT - FUNGIBLE_ASSET_AMOUNT,
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
    let process_state: ProcessState = process.into();

    let expected_final_storage_amount = FUNGIBLE_FAUCET_INITIAL_BALANCE - FUNGIBLE_ASSET_AMOUNT;
    let faucet_reserved_slot_storage_location =
        FAUCET_STORAGE_DATA_SLOT as u32 + NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR;
    let faucet_storage_amount_location = faucet_reserved_slot_storage_location + 3;

    let faucet_storage_amount = process_state
        .get_mem_value(process_state.ctx(), faucet_storage_amount_location)
        .unwrap()
        .as_int();

    assert_eq!(faucet_storage_amount, expected_final_storage_amount);
}

#[test]
fn test_burn_fungible_asset_fails_not_faucet_account() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::burn
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_burn_fungible_asset_inconsistent_faucet_id() {
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ONE,
        false,
    )
    .build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{FUNGIBLE_ASSET_AMOUNT}.0.{suffix}.{prefix}
            call.account::burn
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FUNGIBLE_ASSET_FAUCET_IS_NOT_ORIGIN);
}

#[test]
fn test_burn_fungible_asset_insufficient_input_amount() {
    let tx_context = TransactionContextBuilder::with_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
        ONE,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
    )
    .build();

    let faucet_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1).unwrap();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            exec.prologue::prepare_transaction
            push.{saturating_amount}.0.{suffix}.{prefix}
            call.account::burn
        end
        ",
        prefix = faucet_id.prefix().as_felt(),
        suffix = faucet_id.suffix(),
        saturating_amount = CONSUMED_ASSET_1_AMOUNT + 1
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_VAULT_FUNGIBLE_ASSET_AMOUNT_LESS_THAN_AMOUNT_TO_WITHDRAW);
}

// NON-FUNGIBLE FAUCET BURN TESTS
// ================================================================================================

#[test]
fn test_burn_non_fungible_asset_succeeds() {
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        NonFungibleAsset::mock_issuer().into(),
        ONE,
        false,
    )
    .build();

    let non_fungible_asset_burnt = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);
    let burnt_asset_vault_key = non_fungible_asset_burnt.vault_key();

    let code = format!(
        r#"
        use.std::collections::smt

        use.kernel::account
        use.kernel::asset_vault
        use.kernel::memory
        use.kernel::prologue
        use.test::account->test_account

        begin
            exec.prologue::prepare_transaction

            # add non-fungible asset to the vault
            exec.memory::get_input_vault_root_ptr push.{non_fungible_asset}
            exec.asset_vault::add_non_fungible_asset dropw

            # check that the non-fungible asset is in the faucet SMT
            push.{FAUCET_STORAGE_DATA_SLOT}
            exec.account::get_item
            push.{burnt_asset_vault_key}
            hash
            exec.smt::get
            push.{non_fungible_asset}
            assert_eqw.err="non-fungible asset should be in the faucet SMT"
            dropw

            # check that the non-fungible asset is in the account map
            push.{burnt_asset_vault_key}
            push.{FAUCET_STORAGE_DATA_SLOT}
            exec.account::get_map_item
            push.{non_fungible_asset}
            assert_eqw.err="non-fungible asset should be in the account map"
            dropw

            # burn the non-fungible asset
            push.{non_fungible_asset}
            call.test_account::burn

            # assert the correct asset is returned
            push.{non_fungible_asset}
            assert_eqw.err="burnt asset does not match expected asset"

            # assert the input vault has been updated
            exec.memory::get_input_vault_root_ptr
            push.{non_fungible_asset}
            exec.asset_vault::has_non_fungible_asset
            not assert.err="input vault should not contain burned asset"

            # assert the non-fungible asset has been removed from the faucet SMT
            push.{FAUCET_STORAGE_DATA_SLOT}
            exec.account::get_item
            push.{burnt_asset_vault_key}
            hash
            exec.smt::get
            padw
            assert_eqw.err="burnt asset should have been removed from faucet SMT"
            dropw

            # assert that the non-fungible asset is no longer in the account map
            push.{burnt_asset_vault_key}
            push.{FAUCET_STORAGE_DATA_SLOT}
            exec.account::get_map_item
            padw
            assert_eqw.err="burnt asset should have been removed from map"
            dropw
        end
        "#,
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset_burnt.into()),
        burnt_asset_vault_key = word_to_masm_push_string(&burnt_asset_vault_key),
    );

    tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
}

#[test]
fn test_burn_non_fungible_asset_fails_does_not_exist() {
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        NonFungibleAsset::mock_issuer().into(),
        ONE,
        false,
    )
    .build();

    let non_fungible_asset_burnt = NonFungibleAsset::mock(&[1, 2, 3]);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            # burn asset
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::burn
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset_burnt.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FAUCET_NON_FUNGIBLE_ASSET_TO_BURN_NOT_FOUND);
}

#[test]
fn test_burn_non_fungible_asset_fails_not_faucet_account() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE).build();

    let non_fungible_asset_burnt = NonFungibleAsset::mock(&[1, 2, 3]);

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            # burn asset
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::burn
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset_burnt.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(
        process,
        ERR_FAUCET_BURN_NON_FUNGIBLE_ASSET_CAN_ONLY_BE_CALLED_ON_NON_FUNGIBLE_FAUCET
    );
}

#[test]
fn test_burn_non_fungible_asset_fails_inconsistent_faucet_id() {
    let non_fungible_asset_burnt = NonFungibleAsset::mock(&[1, 2, 3]);

    // Run code from a different non-fungible asset issuer
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET_1,
        ONE,
        false,
    )
    .build();

    let code = format!(
        "
        use.kernel::prologue
        use.test::account

        begin
            # burn asset
            exec.prologue::prepare_transaction
            push.{non_fungible_asset}
            call.account::burn
        end
        ",
        non_fungible_asset = word_to_masm_push_string(&non_fungible_asset_burnt.into())
    );

    let process = tx_context.execute_code_with_assembler(
        &code,
        TransactionKernel::testing_assembler_with_mock_account(),
    );

    assert_execution_error!(process, ERR_FAUCET_NON_FUNGIBLE_ASSET_TO_BURN_NOT_FOUND);
}

// IS NON FUNGIBLE ASSET ISSUED TESTS
// ================================================================================================

#[test]
fn test_is_non_fungible_asset_issued_succeeds() {
    // NON_FUNGIBLE_ASSET_DATA_2 is "issued" during the mock faucet creation, so it is already in
    // the map of issued assets.
    let tx_context = TransactionContextBuilder::with_non_fungible_faucet(
        NonFungibleAsset::mock_issuer().into(),
        ONE,
        false,
    )
    .build();

    let non_fungible_asset_1 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let non_fungible_asset_2 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA_2);

    let code = format!(
        r#"
        use.kernel::prologue
        use.miden::faucet

        begin
            exec.prologue::prepare_transaction

            # check that NON_FUNGIBLE_ASSET_DATA_2 is already issued
            push.{non_fungible_asset_2}
            exec.faucet::is_non_fungible_asset_issued

            # assert that NON_FUNGIBLE_ASSET_DATA_2 is issued
            eq.1 assert.err="non fungible asset data 2 should have been issued"

            # check that NON_FUNGIBLE_ASSET_DATA was not issued yet
            push.{non_fungible_asset_1}
            exec.faucet::is_non_fungible_asset_issued

            # assert that NON_FUNGIBLE_ASSET_DATA is not issued
            eq.0 assert.err="non fungible asset data should have been issued"
        end
        "#,
        non_fungible_asset_1 = word_to_masm_push_string(&non_fungible_asset_1.into()),
        non_fungible_asset_2 = word_to_masm_push_string(&non_fungible_asset_2.into()),
    );

    tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
}

// GET TOTAL ISSUANCE TESTS
// ================================================================================================

#[test]
fn test_get_total_issuance_succeeds() {
    let tx_context = TransactionContextBuilder::with_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ONE,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
    )
    .build();

    let code = format!(
        r#"
        use.kernel::prologue
        use.miden::faucet

        begin
            exec.prologue::prepare_transaction

            # get the fungible faucet balance
            exec.faucet::get_total_issuance
            # => [total_issuance]

            # assert the correct balance is returned
            push.{FUNGIBLE_FAUCET_INITIAL_BALANCE} assert_eq.err="total issuance did not match expected value"
            # => []
        end
        "#,
    );

    tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();
}
