use alloc::{collections::BTreeMap, vec::Vec};

use anyhow::Context;
use miden_lib::{
    account::wallets::BasicWallet,
    errors::tx_kernel_errors::{
        ERR_ACCOUNT_SEED_AND_COMMITMENT_DIGEST_MISMATCH,
        ERR_PROLOGUE_NEW_FUNGIBLE_FAUCET_RESERVED_SLOT_MUST_BE_EMPTY,
        ERR_PROLOGUE_NEW_NON_FUNGIBLE_FAUCET_RESERVED_SLOT_MUST_BE_VALID_EMPTY_SMT,
    },
    transaction::{
        TransactionKernel,
        memory::{
            ACCT_DB_ROOT_PTR, ACCT_ID_PTR, BLOCK_COMMITMENT_PTR, BLOCK_METADATA_PTR,
            BLOCK_NUMBER_IDX, CHAIN_COMMITMENT_PTR, INIT_ACCT_COMMITMENT_PTR, INIT_NONCE_PTR,
            INPUT_NOTE_ARGS_OFFSET, INPUT_NOTE_ASSETS_HASH_OFFSET, INPUT_NOTE_ASSETS_OFFSET,
            INPUT_NOTE_ID_OFFSET, INPUT_NOTE_INPUTS_COMMITMENT_OFFSET, INPUT_NOTE_METADATA_OFFSET,
            INPUT_NOTE_NULLIFIER_SECTION_PTR, INPUT_NOTE_NUM_ASSETS_OFFSET,
            INPUT_NOTE_SCRIPT_ROOT_OFFSET, INPUT_NOTE_SECTION_PTR, INPUT_NOTE_SERIAL_NUM_OFFSET,
            INPUT_NOTES_COMMITMENT_PTR, KERNEL_PROCEDURES_PTR, MemoryOffset,
            NATIVE_ACCT_CODE_COMMITMENT_PTR, NATIVE_ACCT_ID_AND_NONCE_PTR,
            NATIVE_ACCT_PROCEDURES_SECTION_PTR, NATIVE_ACCT_STORAGE_COMMITMENT_PTR,
            NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR, NATIVE_ACCT_VAULT_ROOT_PTR,
            NATIVE_NUM_ACCT_PROCEDURES_PTR, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR, NOTE_ROOT_PTR,
            NULLIFIER_DB_ROOT_PTR, NUM_KERNEL_PROCEDURES_PTR, PARTIAL_BLOCKCHAIN_NUM_LEAVES_PTR,
            PARTIAL_BLOCKCHAIN_PEAKS_PTR, PREV_BLOCK_COMMITMENT_PTR, PROOF_COMMITMENT_PTR,
            PROTOCOL_VERSION_IDX, TIMESTAMP_IDX, TX_COMMITMENT_PTR, TX_KERNEL_COMMITMENT_PTR,
            TX_SCRIPT_ROOT_PTR,
        },
    },
};
use miden_objects::{
    EMPTY_WORD, FieldElement, WORD_SIZE,
    account::{
        Account, AccountBuilder, AccountId, AccountIdVersion, AccountProcedureInfo,
        AccountStorageMode, AccountType, StorageSlot,
    },
    asset::FungibleAsset,
    testing::{
        account_component::AccountMockComponent,
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER,
        },
        constants::FUNGIBLE_FAUCET_INITIAL_BALANCE,
    },
    transaction::{AccountInputs, TransactionArgs, TransactionScript},
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use vm_processor::{AdviceInputs, ExecutionError, Process, Word};

use super::{Felt, ZERO};
use crate::{
    Auth, MockChain, TransactionContext, TransactionContextBuilder, assert_execution_error,
    kernel_tests::tx::read_root_mem_word,
    utils::{create_p2any_note, input_note_data_ptr},
};

#[test]
fn test_transaction_prologue() -> anyhow::Result<()> {
    let mut tx_context = {
        let account = Account::mock(
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
            Felt::ONE,
            Auth::IncrNonce,
            TransactionKernel::testing_assembler(),
        );
        let input_note_1 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);
        let input_note_2 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(100)]);
        let input_note_3 =
            create_p2any_note(ACCOUNT_ID_SENDER.try_into().unwrap(), &[FungibleAsset::mock(111)]);
        TransactionContextBuilder::new(account)
            .extend_input_notes(vec![input_note_1, input_note_2, input_note_3])
            .build()?
    };

    let code = "
        use.kernel::prologue

        begin
            exec.prologue::prepare_transaction
        end
        ";

    let mock_tx_script_code = "
        begin
            nop
        end
        ";

    let mock_tx_script_program = TransactionKernel::assembler()
        .with_debug_mode(true)
        .assemble_program(mock_tx_script_code)
        .unwrap();

    let tx_script = TransactionScript::new(mock_tx_script_program);

    let note_args = [Word::from([91, 91, 91, 91u32]), Word::from([92, 92, 92, 92u32])];

    let note_args_map = BTreeMap::from([
        (tx_context.input_notes().get_note(0).note().id(), note_args[0]),
        (tx_context.input_notes().get_note(1).note().id(), note_args[1]),
    ]);

    let tx_args = TransactionArgs::new(
        tx_context.tx_args().advice_inputs().clone().map,
        Vec::<AccountInputs>::new(),
    )
    .with_tx_script(tx_script)
    .with_note_args(note_args_map);

    tx_context.set_tx_args(tx_args);
    let process = &tx_context.execute_code(code)?;

    global_input_memory_assertions(process, &tx_context);
    block_data_memory_assertions(process, &tx_context);
    partial_blockchain_memory_assertions(process, &tx_context);
    kernel_data_memory_assertions(process);
    account_data_memory_assertions(process, &tx_context);
    input_notes_memory_assertions(process, &tx_context, &note_args);

    Ok(())
}

fn global_input_memory_assertions(process: &Process, inputs: &TransactionContext) {
    assert_eq!(
        read_root_mem_word(&process, BLOCK_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().commitment(),
        "The block commitment should be stored at the BLOCK_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, ACCT_ID_PTR)[0],
        inputs.account().id().suffix(),
        "The account ID prefix should be stored at the ACCT_ID_PTR[0]"
    );
    assert_eq!(
        read_root_mem_word(&process, ACCT_ID_PTR)[1],
        inputs.account().id().prefix().as_felt(),
        "The account ID suffix should be stored at the ACCT_ID_PTR[1]"
    );

    assert_eq!(
        read_root_mem_word(&process, INIT_ACCT_COMMITMENT_PTR),
        inputs.account().commitment(),
        "The account commitment should be stored at the INIT_ACCT_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, INPUT_NOTES_COMMITMENT_PTR),
        inputs.input_notes().commitment(),
        "The nullifier commitment should be stored at the INPUT_NOTES_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, INIT_NONCE_PTR)[0],
        inputs.account().nonce(),
        "The initial nonce should be stored at the INIT_NONCE_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, TX_SCRIPT_ROOT_PTR),
        inputs.tx_args().tx_script().as_ref().unwrap().root(),
        "The transaction script root should be stored at the TX_SCRIPT_ROOT_PTR"
    );
}

fn block_data_memory_assertions(process: &Process, inputs: &TransactionContext) {
    assert_eq!(
        read_root_mem_word(&process, BLOCK_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().commitment(),
        "The block commitment should be stored at the BLOCK_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, PREV_BLOCK_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().prev_block_commitment(),
        "The previous block commitment should be stored at the PARENT_BLOCK_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, CHAIN_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().chain_commitment(),
        "The chain commitment should be stored at the CHAIN_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, ACCT_DB_ROOT_PTR),
        inputs.tx_inputs().block_header().account_root(),
        "The account db root should be stored at the ACCT_DB_ROOT_PRT"
    );

    assert_eq!(
        read_root_mem_word(&process, NULLIFIER_DB_ROOT_PTR),
        inputs.tx_inputs().block_header().nullifier_root(),
        "The nullifier db root should be stored at the NULLIFIER_DB_ROOT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, TX_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().tx_commitment(),
        "The TX commitment should be stored at the TX_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, TX_KERNEL_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().tx_kernel_commitment(),
        "The kernel commitment should be stored at the TX_KERNEL_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, PROOF_COMMITMENT_PTR),
        inputs.tx_inputs().block_header().proof_commitment(),
        "The proof commitment should be stored at the PROOF_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, BLOCK_METADATA_PTR)[BLOCK_NUMBER_IDX],
        inputs.tx_inputs().block_header().block_num().into(),
        "The block number should be stored at BLOCK_METADATA_PTR[BLOCK_NUMBER_IDX]"
    );

    assert_eq!(
        read_root_mem_word(&process, BLOCK_METADATA_PTR)[PROTOCOL_VERSION_IDX],
        inputs.tx_inputs().block_header().version().into(),
        "The protocol version should be stored at BLOCK_METADATA_PTR[PROTOCOL_VERSION_IDX]"
    );

    assert_eq!(
        read_root_mem_word(&process, BLOCK_METADATA_PTR)[TIMESTAMP_IDX],
        inputs.tx_inputs().block_header().timestamp().into(),
        "The timestamp should be stored at BLOCK_METADATA_PTR[TIMESTAMP_IDX]"
    );

    assert_eq!(
        read_root_mem_word(&process, NOTE_ROOT_PTR),
        inputs.tx_inputs().block_header().note_root(),
        "The note root should be stored at the NOTE_ROOT_PTR"
    );
}

fn partial_blockchain_memory_assertions(process: &Process, prepared_tx: &TransactionContext) {
    // update the partial blockchain to point to the block against which this transaction is being
    // executed
    let mut partial_blockchain = prepared_tx.tx_inputs().blockchain().clone();
    partial_blockchain.add_block(prepared_tx.tx_inputs().block_header().clone(), true);

    assert_eq!(
        read_root_mem_word(&process, PARTIAL_BLOCKCHAIN_NUM_LEAVES_PTR)[0],
        Felt::new(partial_blockchain.chain_length().as_u64()),
        "The number of leaves should be stored at the PARTIAL_BLOCKCHAIN_NUM_LEAVES_PTR"
    );

    for (i, peak) in partial_blockchain.peaks().peaks().iter().enumerate() {
        // The peaks should be stored at the PARTIAL_BLOCKCHAIN_PEAKS_PTR
        let peak_idx: u32 = i.try_into().expect(
            "Number of peaks is log2(number_of_leaves), this value won't be larger than 2**32",
        );
        let word_aligned_peak_idx = peak_idx * WORD_SIZE as u32;
        assert_eq!(
            read_root_mem_word(&process, PARTIAL_BLOCKCHAIN_PEAKS_PTR + word_aligned_peak_idx),
            *peak
        );
    }
}

fn kernel_data_memory_assertions(process: &Process) {
    let latest_version_procedures = TransactionKernel::PROCEDURES
        .last()
        .expect("kernel should have at least one version");

    // check that the number of kernel procedures stored in the memory is equal to the number of
    // kernel procedures in the `TransactionKernel` array.
    //
    // By default we check procedures of the latest kernel version
    assert_eq!(
        read_root_mem_word(&process, NUM_KERNEL_PROCEDURES_PTR)[0].as_int(),
        latest_version_procedures.len() as u64,
        "Number of the kernel procedures should be stored at the NUM_KERNEL_PROCEDURES_PTR"
    );

    // check that the hashes of the kernel procedures stored in the memory is equal to the hashes in
    // `TransactionKernel`'s procedures array
    for (i, &proc_hash) in latest_version_procedures.iter().enumerate() {
        assert_eq!(
            read_root_mem_word(&process, KERNEL_PROCEDURES_PTR + (i * WORD_SIZE) as u32),
            proc_hash,
            "hash of kernel procedure at index `{i}` does not match the hash stored in memory"
        );
    }
}

fn account_data_memory_assertions(process: &Process, inputs: &TransactionContext) {
    assert_eq!(
        read_root_mem_word(&process, NATIVE_ACCT_ID_AND_NONCE_PTR),
        Word::new([
            inputs.account().id().suffix(),
            inputs.account().id().prefix().as_felt(),
            ZERO,
            inputs.account().nonce()
        ]),
        "The account ID should be stored at NATIVE_ACCT_ID_AND_NONCE_PTR[0]"
    );

    assert_eq!(
        read_root_mem_word(&process, NATIVE_ACCT_VAULT_ROOT_PTR),
        inputs.account().vault().root(),
        "The account vault root should be stored at NATIVE_ACCT_VAULT_ROOT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, NATIVE_ACCT_STORAGE_COMMITMENT_PTR),
        Word::from(inputs.account().storage().commitment()),
        "The account storage commitment should be stored at NATIVE_ACCT_STORAGE_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, NATIVE_ACCT_CODE_COMMITMENT_PTR),
        inputs.account().code().commitment(),
        "account code commitment should be stored at NATIVE_ACCT_CODE_COMMITMENT_PTR"
    );

    assert_eq!(
        read_root_mem_word(&process, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR),
        Word::from([u16::try_from(inputs.account().storage().slots().len()).unwrap(), 0, 0, 0]),
        "The number of initialised storage slots should be stored at NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR"
    );

    for (i, elements) in inputs
        .account()
        .storage()
        .as_elements()
        .chunks(StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT / 2)
        .enumerate()
    {
        assert_eq!(
            read_root_mem_word(
                &process,
                NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR + (i * WORD_SIZE) as u32
            ),
            Word::try_from(elements).unwrap(),
            "The account storage slots should be stored starting at NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR"
        )
    }

    assert_eq!(
        read_root_mem_word(&process, NATIVE_NUM_ACCT_PROCEDURES_PTR),
        Word::from([u16::try_from(inputs.account().code().procedures().len()).unwrap(), 0, 0, 0]),
        "The number of procedures should be stored at NATIVE_NUM_ACCT_PROCEDURES_PTR"
    );

    for (i, elements) in inputs
        .account()
        .code()
        .as_elements()
        .chunks(AccountProcedureInfo::NUM_ELEMENTS_PER_PROC / 2)
        .enumerate()
    {
        assert_eq!(
            read_root_mem_word(
                &process,
                NATIVE_ACCT_PROCEDURES_SECTION_PTR + (i * WORD_SIZE) as u32
            ),
            Word::try_from(elements).unwrap(),
            "The account procedures and storage offsets should be stored starting at NATIVE_ACCT_PROCEDURES_SECTION_PTR"
        );
    }
}

fn input_notes_memory_assertions(
    process: &Process,
    inputs: &TransactionContext,
    note_args: &[Word],
) {
    assert_eq!(
        read_root_mem_word(&process, INPUT_NOTE_SECTION_PTR),
        Word::from([inputs.input_notes().num_notes(), 0, 0, 0]),
        "number of input notes should be stored at the INPUT_NOTES_OFFSET"
    );

    for (input_note, note_idx) in inputs.input_notes().iter().zip(0_u32..) {
        let note = input_note.note();

        assert_eq!(
            read_root_mem_word(
                &process,
                INPUT_NOTE_NULLIFIER_SECTION_PTR + note_idx * WORD_SIZE as u32
            ),
            note.nullifier().inner(),
            "note nullifier should be computer and stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_ID_OFFSET),
            note.id().inner(),
            "ID hash should be computed and stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_SERIAL_NUM_OFFSET),
            note.serial_num(),
            "note serial num should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_SCRIPT_ROOT_OFFSET),
            note.script().root(),
            "note script root should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_INPUTS_COMMITMENT_OFFSET),
            note.inputs().commitment(),
            "note input commitment should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_ASSETS_HASH_OFFSET),
            note.assets().commitment(),
            "note asset commitment should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_METADATA_OFFSET),
            Word::from(note.metadata()),
            "note metadata should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_ARGS_OFFSET),
            Word::from(note_args[note_idx as usize]),
            "note args should be stored at the correct offset"
        );

        assert_eq!(
            read_note_element(process, note_idx, INPUT_NOTE_NUM_ASSETS_OFFSET),
            Word::from([<u32>::try_from(note.assets().num_assets()).unwrap(), 0, 0, 0]),
            "number of assets should be stored at the correct offset"
        );

        for (asset, asset_idx) in note.assets().iter().cloned().zip(0_u32..) {
            let word: Word = asset.into();
            assert_eq!(
                read_note_element(
                    process,
                    note_idx,
                    INPUT_NOTE_ASSETS_OFFSET + asset_idx * WORD_SIZE as u32
                ),
                word,
                "assets should be stored at (INPUT_NOTES_DATA_OFFSET + note_index * 2048 + 32 + asset_idx * 4)"
            );
        }
    }
}

// ACCOUNT CREATION TESTS
// ================================================================================================

/// Tests that a simple account can be created in a complete transaction execution (not using
/// [`TransactionContext::execute_code`]).
#[test]
fn create_simple_account() -> anyhow::Result<()> {
    let (account, seed) = AccountBuilder::new([6; 32])
        .storage_mode(AccountStorageMode::Public)
        .with_auth_component(Auth::IncrNonce)
        .with_component(AccountMockComponent::new_with_empty_slots(TransactionKernel::assembler())?)
        .build()?;

    let tx = TransactionContextBuilder::new(account)
        .account_seed(Some(seed))
        .build()?
        .execute()
        .context("failed to execute account-creating transaction")?;

    assert_eq!(tx.account_delta().nonce_delta(), Felt::new(1));
    // except for the nonce, the delta should be empty
    assert!(tx.account_delta().storage().is_empty());
    assert!(tx.account_delta().vault().is_empty());
    assert_eq!(tx.final_account().nonce(), Felt::new(1));
    // account commitment should not be the empty word
    assert_ne!(tx.account_delta().commitment(), EMPTY_WORD);

    Ok(())
}

/// Test helper which executes the prologue to check if the creation of the given `account` with its
/// `seed` is valid in the context of the given `mock_chain`.
pub fn create_account_test(
    mock_chain: &MockChain,
    account: Account,
    seed: Word,
) -> Result<Process, ExecutionError> {
    let tx_inputs = mock_chain
        .get_transaction_inputs(account.clone(), Some(seed), &[], &[])
        .unwrap();

    let tx_context = TransactionContextBuilder::new(account)
        .account_seed(Some(seed))
        .tx_inputs(tx_inputs)
        .build()
        .unwrap();

    let code = "
  use.kernel::prologue

  begin
      exec.prologue::prepare_transaction
  end
  ";

    tx_context.execute_code(code)
}

pub fn create_multiple_accounts_test(
    mock_chain: &MockChain,
    storage_mode: AccountStorageMode,
) -> anyhow::Result<()> {
    let mut accounts = Vec::new();

    for account_type in [
        AccountType::RegularAccountImmutableCode,
        AccountType::RegularAccountUpdatableCode,
        AccountType::FungibleFaucet,
        AccountType::NonFungibleFaucet,
    ] {
        let (account, seed) = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
            .account_type(account_type)
            .storage_mode(storage_mode)
            .with_auth_component(Auth::IncrNonce)
            .with_component(
                AccountMockComponent::new_with_slots(
                    TransactionKernel::testing_assembler(),
                    vec![StorageSlot::Value(Word::from([255u32; WORD_SIZE]))],
                )
                .unwrap(),
            )
            .build()
            .context("account build failed")?;

        accounts.push((account, seed));
    }

    for (account, seed) in accounts {
        let account_type = account.account_type();
        create_account_test(mock_chain, account, seed).context(format!(
            "create_multiple_accounts_test test failed for account type {account_type}"
        ))?;
    }

    Ok(())
}

/// Tests that a valid account of each storage mode can be created successfully.
#[test]
pub fn create_accounts_with_all_storage_modes() -> anyhow::Result<()> {
    let mock_chain = MockChain::new();

    create_multiple_accounts_test(&mock_chain, AccountStorageMode::Private)?;

    create_multiple_accounts_test(&mock_chain, AccountStorageMode::Public)?;

    create_multiple_accounts_test(&mock_chain, AccountStorageMode::Network)
}

/// Takes an account with a placeholder ID and returns the same account but with its ID replaced
/// with a newly generated one.
fn compute_valid_account_id(account: Account) -> (Account, Word) {
    let init_seed: [u8; 32] = [5; 32];
    let seed = AccountId::compute_account_seed(
        init_seed,
        account.account_type(),
        AccountStorageMode::Public,
        AccountIdVersion::Version0,
        account.code().commitment(),
        account.storage().commitment(),
    )
    .unwrap();

    let account_id = AccountId::new(
        seed,
        AccountIdVersion::Version0,
        account.code().commitment(),
        account.storage().commitment(),
    )
    .unwrap();

    // Overwrite old ID with generated ID.
    let (_, vault, storage, code, nonce) = account.into_parts();
    let account = Account::from_parts(account_id, vault, storage, code, nonce);

    (account, seed)
}

/// Tests that creating a fungible faucet account with a non-empty initial balance in its reserved
/// slot fails.
#[test]
pub fn create_account_fungible_faucet_invalid_initial_balance() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_next_block()?;

    let account = Account::mock_fungible_faucet(
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ZERO,
        Felt::new(FUNGIBLE_FAUCET_INITIAL_BALANCE),
        TransactionKernel::assembler().with_debug_mode(true),
    );
    let (account, account_seed) = compute_valid_account_id(account);

    let result = create_account_test(&mock_chain, account, account_seed);

    assert_execution_error!(result, ERR_PROLOGUE_NEW_FUNGIBLE_FAUCET_RESERVED_SLOT_MUST_BE_EMPTY);

    Ok(())
}

/// Tests that creating a non fungible faucet account with a non-empty SMT in its reserved slot
/// fails.
#[test]
pub fn create_account_non_fungible_faucet_invalid_initial_reserved_slot() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_next_block()?;

    let account = Account::mock_non_fungible_faucet(
        ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET,
        ZERO,
        false,
        TransactionKernel::assembler().with_debug_mode(true),
    );
    let (account, account_seed) = compute_valid_account_id(account);

    let result = create_account_test(&mock_chain, account, account_seed);

    assert_execution_error!(
        result,
        ERR_PROLOGUE_NEW_NON_FUNGIBLE_FAUCET_RESERVED_SLOT_MUST_BE_VALID_EMPTY_SMT
    );

    Ok(())
}

/// Tests that supplying an invalid seed causes account creation to fail.
#[test]
pub fn create_account_invalid_seed() -> anyhow::Result<()> {
    let mut mock_chain = MockChain::new();
    mock_chain.prove_next_block()?;

    let (account, seed) = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .account_type(AccountType::RegularAccountUpdatableCode)
        .with_auth_component(Auth::IncrNonce)
        .with_component(BasicWallet)
        .build()?;

    let tx_inputs = mock_chain
        .get_transaction_inputs(account.clone(), Some(seed), &[], &[])
        .expect("failed to get transaction inputs from mock chain");

    // override the seed with an invalid seed to ensure the kernel fails
    let account_seed_key = [account.id().suffix(), account.id().prefix().as_felt(), ZERO, ZERO];
    let adv_inputs =
        AdviceInputs::default().with_map([(Word::from(account_seed_key), vec![ZERO; WORD_SIZE])]);

    let tx_context = TransactionContextBuilder::new(account)
        .account_seed(Some(seed))
        .tx_inputs(tx_inputs)
        .extend_advice_inputs(adv_inputs)
        .build()?;

    let code = "
      use.kernel::prologue

      begin
          exec.prologue::prepare_transaction
      end
      ";

    let result = tx_context.execute_code(code);

    assert_execution_error!(result, ERR_ACCOUNT_SEED_AND_COMMITMENT_DIGEST_MISMATCH);

    Ok(())
}

#[test]
fn test_get_blk_version() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;
    let code = "
    use.kernel::memory
    use.kernel::prologue

    begin
        exec.prologue::prepare_transaction
        exec.memory::get_blk_version

        # truncate the stack
        swap drop
    end
    ";

    let process = tx_context.execute_code(code)?;

    assert_eq!(process.stack.get(0), tx_context.tx_inputs().block_header().version().into());

    Ok(())
}

#[test]
fn test_get_blk_timestamp() -> anyhow::Result<()> {
    let tx_context = TransactionContextBuilder::with_existing_mock_account().build()?;
    let code = "
    use.kernel::memory
    use.kernel::prologue

    begin
        exec.prologue::prepare_transaction
        exec.memory::get_blk_timestamp

        # truncate the stack
        swap drop
    end
    ";

    let process = tx_context.execute_code(code)?;

    assert_eq!(process.stack.get(0), tx_context.tx_inputs().block_header().timestamp().into());

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

fn read_note_element(process: &Process, note_idx: u32, offset: MemoryOffset) -> Word {
    read_root_mem_word(&process, input_note_data_ptr(note_idx) + offset)
}
