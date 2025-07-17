use std::collections::BTreeMap;

use anyhow::Context;
use miden_lib::{
    errors::tx_kernel_errors::{
        ERR_ACCOUNT_ID_SUFFIX_LEAST_SIGNIFICANT_BYTE_MUST_BE_ZERO,
        ERR_ACCOUNT_ID_SUFFIX_MOST_SIGNIFICANT_BIT_MUST_BE_ZERO,
        ERR_ACCOUNT_ID_UNKNOWN_STORAGE_MODE, ERR_ACCOUNT_ID_UNKNOWN_VERSION,
        ERR_ACCOUNT_STORAGE_SLOT_INDEX_OUT_OF_BOUNDS, ERR_FAUCET_INVALID_STORAGE_OFFSET,
    },
    transaction::TransactionKernel,
    utils::word_to_masm_push_string,
};
use miden_objects::{
    account::{
        Account, AccountBuilder, AccountCode, AccountComponent, AccountId, AccountIdVersion,
        AccountProcedureInfo, AccountStorage, AccountStorageMode, AccountType, StorageSlot,
    },
    assembly::{
        Library,
        diagnostics::{IntoDiagnostic, NamedSource, Report, WrapErr, miette},
    },
    asset::AssetVault,
    testing::{
        account_component::AccountMockComponent,
        account_id::{
            ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
            ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        },
        storage::STORAGE_LEAVES_2,
    },
    transaction::{ExecutedTransaction, TransactionScript},
};
use miden_tx::TransactionExecutorError;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use vm_processor::{EMPTY_WORD, ExecutionError, Word};

use super::{Felt, ONE, StackInputs, ZERO};
use crate::{
    Auth, MockChain, TransactionContextBuilder, assert_execution_error, executor::CodeExecutor,
};

// ACCOUNT COMMITMENT TESTS
// ================================================================================================

#[test]
pub fn compute_current_commitment() -> miette::Result<()> {
    let account = Account::mock(
        ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
        ONE,
        Auth::IncrNonce,
        TransactionKernel::assembler(),
    );

    // Precompute a commitment to a changed account so we can assert it during tx script execution.
    let mut account_clone = account.clone();
    let key = Word::from([1, 2, 3, 4u32]);
    let value = Word::from([2, 3, 4, 5u32]);
    account_clone.storage_mut().set_map_item(2, key, value).unwrap();
    let expected_commitment = account_clone.commitment();

    let tx_script = format!(
        r#"
        use.miden::prologue
        use.miden::account
        use.test::account->test_account

        begin
            exec.account::get_initial_commitment
            # => [INITIAL_COMMITMENT]

            exec.account::compute_current_commitment
            # => [CURRENT_COMMITMENT, INITIAL_COMMITMENT]

            assert_eqw.err="initial and current commitment should be equal when no changes have been made"
            # => []

            call.test_account::get_storage_commitment
            # => [STORAGE_COMMITMENT0, pad(12)]
            swapdw dropw dropw swapw dropw
            # => [STORAGE_COMMITMENT0]

            # update a value in the storage map
            padw push.0.0.0
            push.{value}
            push.{key}
            push.2
            # => [slot_idx = 2, KEY, VALUE, pad(7)]
            call.test_account::set_map_item
            dropw dropw dropw dropw
            # => [STORAGE_COMMITMENT0]

            # compute the commitment which will recompute the storage commitment
            exec.account::compute_current_commitment
            # => [CURRENT_COMMITMENT, STORAGE_COMMITMENT0]

            push.{expected_commitment}
            assert_eqw.err="current commitment should match expected one"
            # => [STORAGE_COMMITMENT0]

            padw padw padw padw
            call.test_account::get_storage_commitment
            # => [STORAGE_COMMITMENT1, pad(12), STORAGE_COMMITMENT0]
            swapdw dropw dropw swapw dropw
            # => [STORAGE_COMMITMENT1, STORAGE_COMMITMENT0]

            eqw not assert.err="storage commitment should have been updated by compute_current_commitment"
            # => [STORAGE_COMMITMENT1, STORAGE_COMMITMENT0]

            dropw dropw dropw dropw
        end
    "#,
        key = word_to_masm_push_string(&key),
        value = word_to_masm_push_string(&value),
        expected_commitment = word_to_masm_push_string(&expected_commitment),
    );

    let tx_context_builder = TransactionContextBuilder::new(account);
    let tx_script =
        TransactionScript::compile(tx_script, tx_context_builder.assembler()).into_diagnostic()?;
    let tx_context = tx_context_builder
        .tx_script(tx_script)
        .build()
        .map_err(|err| miette::miette!("{err}"))?;

    tx_context.execute().into_diagnostic().wrap_err("failed to execute code")?;

    Ok(())
}

// ACCOUNT CODE TESTS
// ================================================================================================

#[test]
pub fn test_get_code() -> miette::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();
    let code = "
        use.$kernel::prologue
        use.$kernel::account
        begin
            exec.prologue::prepare_transaction
            exec.account::get_code_commitment
            swapw dropw
        end
        ";

    let process = &tx_context.execute_code(code).unwrap();

    assert_eq!(
        process.stack.get_word(0),
        tx_context.account().code().commitment(),
        "obtained code commitment is not equal to the account code commitment",
    );

    Ok(())
}

// ACCOUNT ID TESTS
// ================================================================================================

#[test]
pub fn test_account_type() -> miette::Result<()> {
    let procedures = vec![
        ("is_fungible_faucet", AccountType::FungibleFaucet),
        ("is_non_fungible_faucet", AccountType::NonFungibleFaucet),
        ("is_updatable_account", AccountType::RegularAccountUpdatableCode),
        ("is_immutable_account", AccountType::RegularAccountImmutableCode),
    ];

    let test_cases = [
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET,
    ];

    for (procedure, expected_type) in procedures {
        let mut has_type = false;

        for account_id in test_cases.iter() {
            let account_id = AccountId::try_from(*account_id).unwrap();

            let code = format!(
                "
                use.$kernel::account_id

                begin
                    exec.account_id::{procedure}
                end
                "
            );

            let process = CodeExecutor::with_default_host()
                .stack_inputs(
                    StackInputs::new(vec![account_id.prefix().as_felt()]).into_diagnostic()?,
                )
                .run(&code)?;

            let type_matches = account_id.account_type() == expected_type;
            let expected_result = Felt::from(type_matches);
            has_type |= type_matches;

            assert_eq!(
                process.stack.get(0),
                expected_result,
                "Rust and Masm check on account type diverge. proc: {} account_id: {} account_type: {:?} expected_type: {:?}",
                procedure,
                account_id,
                account_id.account_type(),
                expected_type,
            );
        }

        assert!(has_type, "missing test for type {expected_type:?}");
    }

    Ok(())
}

#[test]
pub fn test_account_validate_id() -> miette::Result<()> {
    let test_cases = [
        (ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE, None),
        (ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE, None),
        (ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, None),
        (ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET, None),
        (
            // Set version to a non-zero value (10).
            ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE | (0x0a << 64),
            Some(ERR_ACCOUNT_ID_UNKNOWN_VERSION),
        ),
        (
            // Set most significant bit to `1`.
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET | (0x80 << 56),
            Some(ERR_ACCOUNT_ID_SUFFIX_MOST_SIGNIFICANT_BIT_MUST_BE_ZERO),
        ),
        (
            // Set storage mode to an unknown value (0b11).
            ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE | (0b11 << (64 + 6)),
            Some(ERR_ACCOUNT_ID_UNKNOWN_STORAGE_MODE),
        ),
        (
            // Set lower 8 bits to a non-zero value (1).
            ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET | 1,
            Some(ERR_ACCOUNT_ID_SUFFIX_LEAST_SIGNIFICANT_BYTE_MUST_BE_ZERO),
        ),
    ];

    for (account_id, expected_error) in test_cases.iter() {
        // Manually split the account ID into prefix and suffix since we can't use AccountId methods
        // on invalid ids.
        let prefix = Felt::try_from((account_id / (1u128 << 64)) as u64).unwrap();
        let suffix = Felt::try_from((account_id % (1u128 << 64)) as u64).unwrap();

        let code = "
            use.$kernel::account_id

            begin
                exec.account_id::validate
            end
            ";

        let result = CodeExecutor::with_default_host()
            .stack_inputs(StackInputs::new(vec![suffix, prefix]).unwrap())
            .run(code);

        match (result, expected_error) {
            (Ok(_), None) => (),
            (Ok(_), Some(err)) => {
                miette::bail!("expected error {err} but validation was successful")
            },
            (Err(ExecutionError::FailedAssertion { err_code, err_msg, .. }), Some(err)) => {
                if err_code != err.code() {
                    miette::bail!(
                        "actual error \"{}\" (code: {err_code}) did not match expected error {err}",
                        err_msg.as_ref().map(AsRef::as_ref).unwrap_or("<no message>")
                    );
                }
            },
            // Construct Reports to get the diagnostics-based error messages.
            (Err(err), None) => {
                return Err(Report::from(err)
                    .context("validation is supposed to succeed but error occurred"));
            },
            (Err(err), Some(_)) => {
                return Err(Report::from(err).context("unexpected different error than expected"));
            },
        }
    }

    Ok(())
}

#[test]
fn test_is_faucet_procedure() -> miette::Result<()> {
    let test_cases = [
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PRIVATE_NON_FUNGIBLE_FAUCET,
    ];

    for account_id in test_cases.iter() {
        let account_id = AccountId::try_from(*account_id).unwrap();

        let code = format!(
            "
            use.$kernel::account_id

            begin
                push.{prefix}
                exec.account_id::is_faucet
                # => [is_faucet, account_id_prefix]

                # truncate the stack
                swap drop
            end
            ",
            prefix = account_id.prefix().as_felt(),
        );

        let process = CodeExecutor::with_default_host()
            .run(&code)
            .wrap_err("failed to execute is_faucet procedure")?;

        let is_faucet = account_id.is_faucet();
        assert_eq!(
            process.stack.get(0),
            Felt::new(is_faucet as u64),
            "Rust and MASM is_faucet diverged for account_id {account_id}"
        );
    }

    Ok(())
}

// ACCOUNT STORAGE TESTS
// ================================================================================================

#[test]
fn test_get_item() -> miette::Result<()> {
    for storage_item in [AccountStorage::mock_item_0(), AccountStorage::mock_item_1()] {
        let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();

        let code = format!(
            "
            use.$kernel::account
            use.$kernel::prologue

            begin
                exec.prologue::prepare_transaction

                # push the account storage item index
                push.{item_index}

                # assert the item value is correct
                exec.account::get_item
                push.{item_value}
                assert_eqw
            end
            ",
            item_index = storage_item.index,
            item_value = word_to_masm_push_string(&storage_item.slot.value())
        );

        tx_context.execute_code(&code).unwrap();
    }

    Ok(())
}

#[test]
fn test_get_map_item() -> miette::Result<()> {
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::assembler(),
                vec![AccountStorage::mock_item_2().slot],
            )
            .unwrap(),
        )
        .build_existing()
        .unwrap();

    let tx_context = TransactionContextBuilder::new(account).build().unwrap();

    for (key, value) in STORAGE_LEAVES_2 {
        let code = format!(
            "
            use.$kernel::prologue

            begin
                exec.prologue::prepare_transaction

                # get the map item
                push.{map_key}
                push.{item_index}
                call.::test::account::get_map_item

                # truncate the stack 
                swapw dropw movup.4 drop
            end
            ",
            item_index = 0,
            map_key = word_to_masm_push_string(&key),
        );

        let process = &tx_context
            .execute_code_with_assembler(
                &code,
                TransactionKernel::testing_assembler_with_mock_account(),
            )
            .unwrap();

        assert_eq!(
            value,
            process.stack.get_word(0),
            "get_map_item result doesn't match the expected value",
        );
        assert_eq!(
            Word::empty(),
            process.stack.get_word(1),
            "The rest of the stack must be cleared",
        );
        assert_eq!(
            Word::empty(),
            process.stack.get_word(2),
            "The rest of the stack must be cleared",
        );
        assert_eq!(
            Word::empty(),
            process.stack.get_word(3),
            "The rest of the stack must be cleared",
        );
    }

    Ok(())
}

#[test]
fn test_get_storage_slot_type() -> miette::Result<()> {
    for storage_item in [
        AccountStorage::mock_item_0(),
        AccountStorage::mock_item_1(),
        AccountStorage::mock_item_2(),
    ] {
        let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();

        let code = format!(
            "
            use.$kernel::account
            use.$kernel::prologue

            begin
                exec.prologue::prepare_transaction

                # push the account storage item index
                push.{item_index}

                # get the type of the respective storage slot
                exec.account::get_storage_slot_type

                # truncate the stack
                swap drop
            end
            ",
            item_index = storage_item.index,
        );

        let process = &tx_context.execute_code(&code).unwrap();

        let storage_slot_type = storage_item.slot.slot_type();

        assert_eq!(storage_slot_type, process.stack.get(0).try_into().unwrap());
        assert_eq!(process.stack.get(1), ZERO, "the rest of the stack is empty");
        assert_eq!(process.stack.get(2), ZERO, "the rest of the stack is empty");
        assert_eq!(process.stack.get(3), ZERO, "the rest of the stack is empty");
        assert_eq!(Word::empty(), process.stack.get_word(1), "the rest of the stack is empty");
        assert_eq!(Word::empty(), process.stack.get_word(2), "the rest of the stack is empty");
        assert_eq!(Word::empty(), process.stack.get_word(3), "the rest of the stack is empty");
    }

    Ok(())
}

#[test]
fn test_set_item() -> miette::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();

    let new_storage_item = Word::from([91, 92, 93, 94u32]);

    let code = format!(
        "
        use.$kernel::account
        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # set the storage item
            push.{new_storage_item}
            push.{new_storage_item_index}
            exec.account::set_item

            # assert old value was correctly returned
            push.1.2.3.4 assert_eqw

            # assert new value has been correctly set
            push.{new_storage_item_index}
            exec.account::get_item
            push.{new_storage_item}
            assert_eqw
        end
        ",
        new_storage_item = word_to_masm_push_string(&new_storage_item),
        new_storage_item_index = 0,
    );

    tx_context.execute_code(&code).unwrap();

    Ok(())
}

#[test]
fn test_set_map_item() -> miette::Result<()> {
    let (new_key, new_value) =
        (Word::from([109, 110, 111, 112u32]), Word::from([9, 10, 11, 12u32]));

    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::assembler(),
                vec![AccountStorage::mock_item_2().slot],
            )
            .unwrap(),
        )
        .build_existing()
        .unwrap();

    let tx_context = TransactionContextBuilder::new(account).build().unwrap();
    let storage_item = AccountStorage::mock_item_2();

    let code = format!(
        "
        use.std::sys

        use.test::account
        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # set the map item
            push.{new_value}
            push.{new_key}
            push.{item_index}
            call.account::set_map_item

            # double check that on storage slot is indeed the new map
            push.{item_index}
            call.account::get_item

            # truncate the stack
            exec.sys::truncate_stack
        end
        ",
        item_index = 0,
        new_key = word_to_masm_push_string(&new_key),
        new_value = word_to_masm_push_string(&new_value),
    );

    let process = &tx_context
        .execute_code_with_assembler(
            &code,
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();

    let mut new_storage_map = AccountStorage::mock_map();
    new_storage_map.insert(new_key, new_value);

    assert_eq!(
        new_storage_map.root(),
        process.stack.get_word(0),
        "get_item must return the new updated value",
    );
    assert_eq!(
        storage_item.slot.value(),
        process.stack.get_word(1),
        "The original value stored in the map doesn't match the expected value",
    );

    Ok(())
}

#[test]
fn test_account_component_storage_offset() -> miette::Result<()> {
    // setup assembler
    let assembler = TransactionKernel::testing_assembler();

    // The following code will execute the following logic that will be asserted during the test:
    //
    // 1. foo_write will set word [1, 2, 3, 4] in storage at location 0 (0 offset by 0)
    // 2. foo_read will read word [1, 2, 3, 4] in storage from location 0 (0 offset by 0)
    // 3. bar_write will set word [5, 6, 7, 8] in storage at location 1 (0 offset by 1)
    // 4. bar_read will read word [5, 6, 7, 8] in storage from location 1 (0 offset by 1)
    //
    // We will then assert that we are able to retrieve the correct elements from storage
    // insuring consistent "set" and "get" using offsets.
    let source_code_component1 = "
        use.miden::account

        export.foo_write
            push.1.2.3.4.0
            exec.account::set_item

            dropw
        end

        export.foo_read
            push.0
            exec.account::get_item
            push.1.2.3.4 eqw assert

            dropw dropw
        end
    ";

    let source_code_component2 = "
        use.miden::account

        export.bar_write
            push.5.6.7.8.0
            exec.account::set_item

            dropw
        end

        export.bar_read
            push.0
            exec.account::get_item
            push.5.6.7.8 eqw assert

            dropw dropw
        end
    ";

    // Compile source code to find MAST roots of procedures.
    let code1 = assembler.clone().assemble_library([source_code_component1]).unwrap();
    let code2 = assembler.clone().assemble_library([source_code_component2]).unwrap();
    let find_procedure_digest_by_name = |name: &str, lib: &Library| {
        lib.exports().find_map(|export| {
            if export.name.as_str() == name {
                Some(lib.mast_forest()[lib.get_export_node_id(export)].digest())
            } else {
                None
            }
        })
    };

    let foo_write = find_procedure_digest_by_name("foo_write", &code1).unwrap();
    let foo_read = find_procedure_digest_by_name("foo_read", &code1).unwrap();
    let bar_write = find_procedure_digest_by_name("bar_write", &code2).unwrap();
    let bar_read = find_procedure_digest_by_name("bar_read", &code2).unwrap();

    // Compile source code into components.
    let component1 = AccountComponent::compile(
        source_code_component1,
        assembler.clone(),
        vec![StorageSlot::Value(Word::empty())],
    )
    .unwrap()
    .with_supported_type(AccountType::RegularAccountUpdatableCode);

    let component2 = AccountComponent::compile(
        source_code_component2,
        assembler.clone(),
        vec![StorageSlot::Value(Word::empty())],
    )
    .unwrap()
    .with_supported_type(AccountType::RegularAccountUpdatableCode);

    let mut account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(component1)
        .with_component(component2)
        .build_existing()
        .unwrap();

    // Assert that the storage offset and size have been set correctly.
    for (procedure_digest, expected_offset, expected_size) in
        [(foo_write, 0, 1), (foo_read, 0, 1), (bar_write, 1, 1), (bar_read, 1, 1)]
    {
        let procedure_info = account
            .code()
            .procedures()
            .iter()
            .find(|proc| proc.mast_root() == &procedure_digest)
            .unwrap();
        assert_eq!(
            procedure_info.storage_offset(),
            expected_offset,
            "failed for procedure {procedure_digest}"
        );
        assert_eq!(
            procedure_info.storage_size(),
            expected_size,
            "failed for procedure {procedure_digest}"
        );
    }

    // setup transaction script
    let tx_script_source_code = format!(
        "
    begin
        call.{foo_write}
        call.{foo_read}
        call.{bar_write}
        call.{bar_read}
    end
    "
    );
    let tx_script_program = assembler.assemble_program(tx_script_source_code).unwrap();
    let tx_script = TransactionScript::new(tx_script_program);

    // setup transaction context
    let tx_context = TransactionContextBuilder::new(account.clone())
        .tx_script(tx_script)
        .build()
        .unwrap();

    // execute code in context
    let tx = tx_context.execute().into_diagnostic()?;
    account.apply_delta(tx.account_delta()).unwrap();

    // assert that elements have been set at the correct locations in storage
    assert_eq!(account.storage().get_item(0).unwrap(), Word::from([1, 2, 3, 4u32]));

    assert_eq!(account.storage().get_item(1).unwrap(), Word::from([5, 6, 7, 8u32]));

    Ok(())
}

/// Tests that we can successfully create regular and faucet accounts with empty storage.
#[test]
fn create_account_with_empty_storage_slots() -> anyhow::Result<()> {
    for account_type in [AccountType::FungibleFaucet, AccountType::RegularAccountUpdatableCode] {
        let (account, seed) = AccountBuilder::new([5; 32])
            .account_type(account_type)
            .with_auth_component(Auth::IncrNonce)
            .with_component(
                AccountMockComponent::new_with_empty_slots(TransactionKernel::testing_assembler())
                    .unwrap(),
            )
            .build()
            .context("failed to build account")?;

        TransactionContextBuilder::new(account)
            .account_seed(Some(seed))
            .build()?
            .execute()?;
    }

    Ok(())
}

fn create_procedure_metadata_test_account(
    account_type: AccountType,
    storage_offset: u8,
    storage_size: u8,
) -> anyhow::Result<Result<ExecutedTransaction, ExecutionError>> {
    let mock_chain = MockChain::new();

    let version = AccountIdVersion::Version0;

    let mock_code = AccountCode::mock();
    let code = AccountCode::from_parts(
        mock_code.mast(),
        mock_code
            .mast()
            .procedure_digests()
            .map(|mast_root| {
                AccountProcedureInfo::new(mast_root, storage_offset, storage_size).unwrap()
            })
            .collect(),
    );

    let storage = AccountStorage::new(vec![StorageSlot::Value(EMPTY_WORD)]).unwrap();

    let seed = AccountId::compute_account_seed(
        [9; 32],
        account_type,
        AccountStorageMode::Private,
        version,
        code.commitment(),
        storage.commitment(),
    )
    .context("failed to compute seed")?;
    let id = AccountId::new(seed, version, code.commitment(), storage.commitment())
        .context("failed to compute ID")?;

    let account = Account::from_parts(id, AssetVault::default(), storage, code, Felt::from(0u32));

    let tx_inputs = mock_chain.get_transaction_inputs(account.clone(), Some(seed), &[], &[])?;
    let tx_context = TransactionContextBuilder::new(account)
        .account_seed(Some(seed))
        .tx_inputs(tx_inputs)
        .build()?;

    let result = tx_context.execute().map_err(|err| {
        let TransactionExecutorError::TransactionProgramExecutionFailed(exec_err) = err else {
            panic!("should have received an execution error");
        };

        exec_err
    });

    Ok(result)
}

/// Tests that creating an account whose procedure accesses the reserved faucet storage slot fails.
#[test]
fn creating_faucet_account_with_procedure_accessing_reserved_slot_fails() -> anyhow::Result<()> {
    // Set offset to 0 for a faucet which should be disallowed.
    let execution_res = create_procedure_metadata_test_account(AccountType::FungibleFaucet, 0, 1)
        .context("failed to create test account")?;

    assert_execution_error!(execution_res, ERR_FAUCET_INVALID_STORAGE_OFFSET);

    Ok(())
}

/// Tests that creating a faucet whose procedure offset+size is out of bounds fails.
#[test]
fn creating_faucet_with_procedure_offset_plus_size_out_of_bounds_fails() -> anyhow::Result<()> {
    // Set offset to lowest allowed value 1 and size to 1 while number of slots is 1 which should
    // result in an out of bounds error.
    let execution_res = create_procedure_metadata_test_account(AccountType::FungibleFaucet, 1, 1)
        .context("failed to create test account")?;

    assert_execution_error!(execution_res, ERR_ACCOUNT_STORAGE_SLOT_INDEX_OUT_OF_BOUNDS);

    // Set offset to 2 while number of slots is 1 which should result in an out of bounds error.
    let execution_res = create_procedure_metadata_test_account(AccountType::FungibleFaucet, 2, 1)
        .context("failed to create test account")?;

    assert_execution_error!(execution_res, ERR_ACCOUNT_STORAGE_SLOT_INDEX_OUT_OF_BOUNDS);

    Ok(())
}

/// Tests that creating an account whose procedure offset+size is out of bounds fails.
#[test]
fn creating_account_with_procedure_offset_plus_size_out_of_bounds_fails() -> anyhow::Result<()> {
    // Set size to 2 while number of slots is 1 which should result in an out of bounds error.
    let execution_res =
        create_procedure_metadata_test_account(AccountType::RegularAccountImmutableCode, 0, 2)
            .context("failed to create test account")?;

    assert_execution_error!(execution_res, ERR_ACCOUNT_STORAGE_SLOT_INDEX_OUT_OF_BOUNDS);

    // Set offset to 2 while number of slots is 1 which should result in an out of bounds error.
    let execution_res =
        create_procedure_metadata_test_account(AccountType::RegularAccountImmutableCode, 2, 1)
            .context("failed to create test account")?;

    assert_execution_error!(execution_res, ERR_ACCOUNT_STORAGE_SLOT_INDEX_OUT_OF_BOUNDS);

    Ok(())
}

// ACCOUNT VAULT TESTS
// ================================================================================================

#[test]
fn test_get_vault_root() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;

    let account = tx_context.account();
    let code = format!(
        "
        use.miden::account
        use.$kernel::prologue

        begin
            exec.prologue::prepare_transaction

            # push the new storage item onto the stack
            exec.account::get_vault_root
            push.{expected_vault_root}
            assert_eqw
        end
        ",
        expected_vault_root = word_to_masm_push_string(&account.vault().root()),
    );

    tx_context.execute_code(&code)?;
    Ok(())
}

// PROCEDURE AUTHENTICATION TESTS
// ================================================================================================

#[test]
fn test_authenticate_and_track_procedure() -> miette::Result<()> {
    let mock_component =
        AccountMockComponent::new_with_empty_slots(TransactionKernel::assembler()).unwrap();

    let account_code = AccountCode::from_components(
        &[Auth::IncrNonce.into(), mock_component.into()],
        AccountType::RegularAccountUpdatableCode,
    )
    .unwrap();

    let tc_0 = *account_code.procedures()[1].mast_root();
    let tc_1 = *account_code.procedures()[2].mast_root();
    let tc_2 = *account_code.procedures()[3].mast_root();

    let test_cases =
        vec![(tc_0, true), (tc_1, true), (tc_2, true), (Word::from([1, 0, 1, 0u32]), false)];

    for (root, valid) in test_cases.into_iter() {
        let tx_context = TransactionContextBuilder::with_existing_mock_account().build().unwrap();

        let code = format!(
            "
            use.$kernel::account
            use.$kernel::prologue

            begin
                exec.prologue::prepare_transaction

                # authenticate procedure
                push.{root}
                exec.account::authenticate_and_track_procedure

                # truncate the stack
                dropw
            end
            ",
            root = word_to_masm_push_string(&root)
        );

        // Execution of this code will return an EventError(UnknownAccountProcedure) for procs
        // that are not in the advice provider.
        let process = tx_context.execute_code(&code);

        match valid {
            true => assert!(process.is_ok(), "A valid procedure must successfully authenticate"),
            false => assert!(process.is_err(), "An invalid procedure should fail to authenticate"),
        }
    }

    Ok(())
}

// PROCEDURE INTROSPECTION TESTS
// ================================================================================================

#[test]
fn test_was_procedure_called() -> miette::Result<()> {
    // Create a standard account using the mock component
    let mock_component = AccountMockComponent::new_with_slots(
        TransactionKernel::assembler(),
        AccountStorage::mock_storage_slots(),
    )
    .unwrap();
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(mock_component)
        .build_existing()
        .unwrap();

    // Create a transaction script that:
    // 1. Checks that get_item hasn't been called yet
    // 2. Calls get_item from the mock account
    // 3. Checks that get_item has been called
    // 4. Calls get_item **again**
    // 5. Checks that `was_procedure_called` returns `true`
    let tx_script_code = r#"
        use.test::account->test_account
        use.miden::account

        begin
            # First check that get_item procedure hasn't been called yet
            procref.test_account::get_item
            exec.account::was_procedure_called
            assertz.err="procedure should not have been called"

            # Call the procedure first time
            push.0
            call.test_account::get_item dropw
            # => []

            procref.test_account::get_item
            exec.account::was_procedure_called
            assert.err="procedure should have been called"

            # Call the procedure second time
            push.0
            call.test_account::get_item dropw

            procref.test_account::get_item
            exec.account::was_procedure_called
            assert.err="2nd call should not change the was_called flag"
        end
        "#;

    // Compile the transaction script using the testing assembler with mock account
    let assembler = TransactionKernel::testing_assembler_with_mock_account();
    let tx_script = TransactionScript::new(
        assembler
            .assemble_program(tx_script_code)
            .wrap_err("Failed to compile transaction script")?,
    );

    // Create transaction context and execute
    let tx_context = TransactionContextBuilder::new(account).tx_script(tx_script).build().unwrap();

    tx_context
        .execute()
        .into_diagnostic()
        .wrap_err("Failed to execute transaction")?;

    Ok(())
}

/// Tests that an account can call code in a custom library when loading that library into the
/// executor.
///
/// The call chain and dependency graph in this test is:
/// `tx script -> account code -> external library`
#[test]
fn transaction_executor_account_code_using_custom_library() -> miette::Result<()> {
    const EXTERNAL_LIBRARY_CODE: &str = r#"
      use.miden::account

      export.external_setter
        push.2.3.4.5
        push.0
        exec.account::set_item
        dropw dropw
      end"#;

    const ACCOUNT_COMPONENT_CODE: &str = "
      use.external_library::external_module

      export.custom_setter
        exec.external_module::external_setter
      end";

    let external_library_source =
        NamedSource::new("external_library::external_module", EXTERNAL_LIBRARY_CODE);
    let external_library =
        TransactionKernel::assembler().assemble_library([external_library_source])?;

    let mut assembler = TransactionKernel::testing_assembler_with_mock_account();
    assembler.link_static_library(&external_library)?;

    let account_component_source =
        NamedSource::new("account_component::account_module", ACCOUNT_COMPONENT_CODE);
    let account_component_lib =
        assembler.clone().assemble_library([account_component_source]).unwrap();

    let tx_script_src = "\
          use.account_component::account_module

          begin
            call.account_module::custom_setter
          end";

    let account_component =
        AccountComponent::new(account_component_lib.clone(), AccountStorage::mock_storage_slots())
            .into_diagnostic()?
            .with_supports_all_types();

    // Build an existing account with nonce 1.
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_auth_component(Auth::IncrNonce)
        .with_component(account_component)
        .build_existing()
        .into_diagnostic()?;

    let tx_script = TransactionScript::compile(
        tx_script_src,
        // Add the account component library since the transaction script is calling the account's
        // procedure.
        // Because the account code is provided by the account itself in the transaction, it can be
        // linked dynamically.
        TransactionKernel::assembler().with_dynamic_library(&account_component_lib)?,
    )
    .into_diagnostic()?;

    let tx_context = TransactionContextBuilder::new(native_account.clone())
        .tx_script(tx_script)
        .build()
        .unwrap();

    let executed_tx = tx_context.execute().into_diagnostic()?;

    // Account's initial nonce of 1 should have been incremented by 1.
    assert_eq!(executed_tx.account_delta().nonce_delta(), Felt::new(1));

    // Make sure that account storage has been updated as per the tx script call.
    assert_eq!(
        *executed_tx.account_delta().storage().values(),
        BTreeMap::from([(0, Word::from([2, 3, 4, 5u32]))]),
    );
    Ok(())
}
