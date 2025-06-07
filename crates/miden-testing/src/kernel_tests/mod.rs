use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    sync::Arc,
    vec::Vec,
};

use ::assembly::{
    LibraryPath,
    ast::{Module, ModuleKind},
};
use anyhow::Context;
use assert_matches::assert_matches;
use miden_lib::{
    note::{create_p2id_note, create_p2idr_note},
    transaction::TransactionKernel,
    utils::word_to_masm_push_string,
};
use miden_objects::{
    Felt, FieldElement, Hasher, MIN_PROOF_SECURITY_LEVEL, TransactionScriptError, Word,
    account::{Account, AccountBuilder, AccountComponent, AccountId, AccountStorage, StorageSlot},
    assembly::DefaultSourceManager,
    asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset},
    block::BlockNumber,
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteExecutionMode, NoteHeader, NoteId, NoteInputs,
        NoteMetadata, NoteRecipient, NoteScript, NoteTag, NoteType,
    },
    testing::{
        account_component::AccountMockComponent,
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
            ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE, ACCOUNT_ID_SENDER,
        },
        constants::{FUNGIBLE_ASSET_AMOUNT, NON_FUNGIBLE_ASSET_DATA},
        note::{DEFAULT_NOTE_CODE, NoteBuilder},
        storage::{STORAGE_INDEX_0, STORAGE_INDEX_2},
    },
    transaction::{OutputNote, ProvenTransaction, TransactionScript},
};
use miden_tx::{
    LocalTransactionProver, NoteAccountExecution, NoteConsumptionChecker, ProvingOptions,
    TransactionExecutor, TransactionExecutorError, TransactionHost, TransactionMastStore,
    TransactionProver, TransactionVerifier,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use vm_processor::{
    Digest, ExecutionError, MemAdviceProvider, ONE,
    crypto::RpoRandomCoin,
    utils::{Deserializable, Serializable},
};

use crate::TransactionContextBuilder;

mod batch;
mod block;
mod tx;

// TESTS
// ================================================================================================

#[test]
fn transaction_executor_witness() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();

    let executed_transaction = tx_context.execute().unwrap();

    let tx_inputs = executed_transaction.tx_inputs();
    let tx_args = executed_transaction.tx_args();

    // use the witness to execute the transaction again
    let (stack_inputs, advice_inputs) = TransactionKernel::prepare_inputs(
        tx_inputs,
        tx_args,
        Some(executed_transaction.advice_witness().clone()),
    )
    .unwrap();
    let mem_advice_provider: MemAdviceProvider = advice_inputs.into_advice_inputs().into();

    // load account/note/tx_script MAST to the mast_store
    let mast_store = Arc::new(TransactionMastStore::new());
    mast_store.load_transaction_code(tx_inputs.account().code(), tx_inputs.input_notes(), tx_args);

    let mut host: TransactionHost<MemAdviceProvider> = TransactionHost::new(
        tx_inputs.account().into(),
        mem_advice_provider,
        mast_store,
        None,
        BTreeSet::new(),
    )
    .unwrap();
    let result = vm_processor::execute(
        &TransactionKernel::main(),
        stack_inputs,
        &mut host,
        Default::default(),
        Arc::new(DefaultSourceManager::default()),
    )
    .unwrap();

    let (advice_provider, _, output_notes, _signatures, _tx_progress) = host.into_parts();
    let (_, map, _) = advice_provider.into_parts();
    let tx_outputs = TransactionKernel::from_transaction_parts(
        result.stack_outputs(),
        &map.into(),
        output_notes,
    )
    .unwrap();

    assert_eq!(
        executed_transaction.final_account().commitment(),
        tx_outputs.account.commitment()
    );
    assert_eq!(executed_transaction.output_notes(), &tx_outputs.output_notes);
}

#[test]
fn executed_transaction_account_delta_new() {
    let account_assets = AssetVault::mock().assets().collect::<Vec<Asset>>();
    let account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_component(
            AccountMockComponent::new_with_slots(
                TransactionKernel::testing_assembler(),
                AccountStorage::mock_storage_slots(),
            )
            .unwrap(),
        )
        .with_assets(account_assets)
        .build_existing()
        .unwrap();

    // updated storage
    let updated_slot_value = [Felt::new(7), Felt::new(9), Felt::new(11), Felt::new(13)];

    // updated storage map
    let updated_map_key = [Felt::new(14), Felt::new(15), Felt::new(16), Felt::new(17)];
    let updated_map_value = [Felt::new(18), Felt::new(19), Felt::new(20), Felt::new(21)];

    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let removed_assets = [removed_asset_1, removed_asset_2, removed_asset_3];

    let tag1 = NoteTag::from_account_id(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        NoteExecutionMode::Local,
    )
    .unwrap();
    let tag2 = NoteTag::for_local_use_case(0, 0).unwrap();
    let tag3 = NoteTag::for_local_use_case(0, 0).unwrap();
    let tags = [tag1, tag2, tag3];

    let aux_array = [Felt::new(27), Felt::new(28), Felt::new(29)];

    let note_types = [NoteType::Private; 3];

    tag1.validate(NoteType::Private)
        .expect("note tag 1 should support private notes");
    tag2.validate(NoteType::Private)
        .expect("note tag 2 should support private notes");
    tag3.validate(NoteType::Private)
        .expect("note tag 3 should support private notes");

    let execution_hint_1 = Felt::from(NoteExecutionHint::always());
    let execution_hint_2 = Felt::from(NoteExecutionHint::none());
    let execution_hint_3 = Felt::from(NoteExecutionHint::on_block_slot(1, 1, 1));
    let hints = [execution_hint_1, execution_hint_2, execution_hint_3];

    let mut send_asset_script = String::new();
    for i in 0..3 {
        send_asset_script.push_str(&format!(
            "
            ### note {i}
            # prepare the stack for a new note creation
            push.0.1.2.3           # recipient
            push.{EXECUTION_HINT}  # note_execution_hint
            push.{NOTETYPE}        # note_type
            push.{aux}             # aux
            push.{tag}             # tag
            # => [tag, aux, note_type, execution_hint, RECIPIENT]

            # pad the stack before calling the `create_note`
            padw padw swapdw
            # => [tag, aux, note_type, execution_hint, RECIPIENT, pad(8)]

            # create the note
            call.::miden::contracts::wallets::basic::create_note
            # => [note_idx, pad(15)]

            # move an asset to the created note to partially deplete fungible asset balance
            swapw dropw push.{REMOVED_ASSET}
            call.::miden::contracts::wallets::basic::move_asset_to_note
            # => [ASSET, note_idx, pad(11)]

            # clear the stack
            dropw dropw dropw dropw
        ",
            EXECUTION_HINT = hints[i],
            NOTETYPE = note_types[i] as u8,
            aux = aux_array[i],
            tag = tags[i],
            REMOVED_ASSET = word_to_masm_push_string(&Word::from(removed_assets[i]))
        ));
    }

    let tx_script_src = format!(
        "\
        use.test::account

        ## TRANSACTION SCRIPT
        ## ========================================================================================
        begin
            ## Update account storage item
            ## ------------------------------------------------------------------------------------
            # push a new value for the storage slot onto the stack             
            push.{UPDATED_SLOT_VALUE}
            # => [13, 11, 9, 7]

            # get the index of account storage slot
            push.{STORAGE_INDEX_0}
            # => [idx, 13, 11, 9, 7]
            # update the storage value
            call.account::set_item dropw dropw
            # => []

            ## Update account storage map
            ## ------------------------------------------------------------------------------------
            # push a new VALUE for the storage map onto the stack             
            push.{UPDATED_MAP_VALUE}
            # => [18, 19, 20, 21]

            # push a new KEY for the storage map onto the stack
            push.{UPDATED_MAP_KEY}
            # => [14, 15, 16, 17, 18, 19, 20, 21]

            # get the index of account storage slot
            push.{STORAGE_INDEX_2}
            # => [idx, 14, 15, 16, 17, 18, 19, 20, 21]

            # update the storage value
            call.account::set_map_item dropw dropw dropw
            # => []

            ## Send some assets from the account vault
            ## ------------------------------------------------------------------------------------
            {send_asset_script}

            ## Update the account nonce
            ## ------------------------------------------------------------------------------------
            push.1 call.account::incr_nonce drop             
            # => []
        end
    ",
        UPDATED_SLOT_VALUE = word_to_masm_push_string(&Word::from(updated_slot_value)),
        UPDATED_MAP_VALUE = word_to_masm_push_string(&Word::from(updated_map_value)),
        UPDATED_MAP_KEY = word_to_masm_push_string(&Word::from(updated_map_key)),
    );

    let tx_script = TransactionScript::compile(
        tx_script_src,
        [],
        TransactionKernel::testing_assembler_with_mock_account(),
    )
    .unwrap();

    let tx_context = TransactionContextBuilder::new(account)
        .with_mock_notes_preserved_with_account_vault_delta()
        .tx_script(tx_script)
        .build();

    // Storing assets that will be added to assert correctness later
    let added_assets = tx_context
        .tx_inputs()
        .input_notes()
        .iter()
        .find_map(|n| {
            let assets = n.note().assets();
            (assets.num_assets() == 3).then(|| assets.iter().cloned().collect::<Vec<_>>())
        })
        .unwrap();

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness
    let executed_transaction = tx_context.execute().unwrap();

    // nonce delta
    // --------------------------------------------------------------------------------------------
    assert_eq!(executed_transaction.account_delta().nonce(), Some(Felt::new(2)));

    // storage delta
    // --------------------------------------------------------------------------------------------
    // We expect one updated item and one updated map
    assert_eq!(executed_transaction.account_delta().storage().values().len(), 1);
    assert_eq!(
        executed_transaction.account_delta().storage().values().get(&STORAGE_INDEX_0),
        Some(&updated_slot_value)
    );

    assert_eq!(executed_transaction.account_delta().storage().maps().len(), 1);
    assert_eq!(
        executed_transaction
            .account_delta()
            .storage()
            .maps()
            .get(&STORAGE_INDEX_2)
            .unwrap()
            .leaves(),
        &Some((updated_map_key.into(), updated_map_value))
            .into_iter()
            .collect::<BTreeMap<Digest, _>>()
    );

    // vault delta
    // --------------------------------------------------------------------------------------------
    // assert that added assets are tracked
    assert!(
        executed_transaction
            .account_delta()
            .vault()
            .added_assets()
            .all(|x| added_assets.contains(&x))
    );
    assert_eq!(
        added_assets.len(),
        executed_transaction.account_delta().vault().added_assets().count()
    );

    // assert that removed assets are tracked
    assert!(
        executed_transaction
            .account_delta()
            .vault()
            .removed_assets()
            .all(|x| removed_assets.contains(&x))
    );
    assert_eq!(
        removed_assets.len(),
        executed_transaction.account_delta().vault().removed_assets().count()
    );
}

#[test]
fn test_empty_delta_nonce_update() {
    let tx_script_src = "
        use.test::account
        begin
            push.1

            call.account::incr_nonce
            # => [0, 1]

            drop drop
            # => []
        end
    ";

    let tx_script = TransactionScript::compile(
        tx_script_src,
        [],
        TransactionKernel::testing_assembler_with_mock_account(),
    )
    .unwrap();

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .tx_script(tx_script)
        .build();

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness
    let executed_transaction = tx_context.execute().unwrap();

    // nonce delta
    // --------------------------------------------------------------------------------------------
    assert_eq!(executed_transaction.account_delta().nonce(), Some(Felt::new(2)));

    // storage delta
    // --------------------------------------------------------------------------------------------
    // We expect one updated item and one updated map
    assert_eq!(executed_transaction.account_delta().storage().values().len(), 0);

    assert_eq!(executed_transaction.account_delta().storage().maps().len(), 0);
}

#[test]
fn test_send_note_proc() {
    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);

    let tag = NoteTag::from_account_id(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        NoteExecutionMode::Local,
    )
    .unwrap();
    let aux = Felt::new(27);
    let note_type = NoteType::Private;

    tag.validate(note_type).expect("note tag should support private notes");

    // prepare the asset vector to be removed for each test variant
    let assets_matrix = vec![
        vec![],
        vec![removed_asset_1],
        vec![removed_asset_1, removed_asset_2],
        vec![removed_asset_1, removed_asset_2, removed_asset_3],
    ];

    for (idx, removed_assets) in assets_matrix.into_iter().enumerate() {
        // Prepare the string containing the procedures required for adding assets to the note.
        // Depending on the number of the assets to remove, the resulting string will be extended
        // with the corresponding number of procedure "blocks"
        let mut assets_to_remove = String::new();
        for asset in removed_assets.iter() {
            assets_to_remove.push_str(&format!(
                "\n
            # prepare the stack for the next call
            dropw

            # push the asset to be removed
            push.{ASSET}
            # => [ASSET, note_idx, GARBAGE(11)]

            call.wallet::move_asset_to_note
            # => [ASSET, note_idx, GARBAGE(11)]\n",
                ASSET = word_to_masm_push_string(&asset.into())
            ))
        }

        let tx_script_src = format!(
            "\
            use.miden::contracts::wallets::basic->wallet
            use.test::account

            ## TRANSACTION SCRIPT
            ## ========================================================================================
            begin
                # prepare the values for note creation
                push.1.2.3.4      # recipient
                push.1            # note_execution_hint (NoteExecutionHint::Always)
                push.{note_type}  # note_type
                push.{aux}        # aux
                push.{tag}        # tag
                # => [tag, aux, note_type, RECIPIENT, ...]

                # pad the stack with zeros before calling the `create_note`.
                padw padw swapdw
                # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8) ...]

                call.wallet::create_note
                # => [note_idx, GARBAGE(15)]

                movdn.4
                # => [GARBAGE(4), note_idx, GARBAGE(11)]

                {assets_to_remove}

                dropw dropw dropw dropw

                ## Update the account nonce
                ## ------------------------------------------------------------------------------------
                push.1 call.account::incr_nonce drop
                # => []
            end
        ",
            note_type = note_type as u8,
        );

        let tx_script = TransactionScript::compile(
            tx_script_src,
            [],
            TransactionKernel::testing_assembler_with_mock_account(),
        )
        .unwrap();

        let tx_context = TransactionContextBuilder::with_standard_account(ONE)
            .tx_script(tx_script)
            .with_mock_notes_preserved_with_account_vault_delta()
            .build();

        // expected delta
        // --------------------------------------------------------------------------------------------
        // execute the transaction and get the witness
        let executed_transaction = tx_context
            .execute()
            .unwrap_or_else(|_| panic!("test failed in iteration {idx}"));

        // nonce delta
        // --------------------------------------------------------------------------------------------
        assert_eq!(executed_transaction.account_delta().nonce(), Some(Felt::new(2)));

        // vault delta
        // --------------------------------------------------------------------------------------------
        // assert that removed assets are tracked
        assert!(
            executed_transaction
                .account_delta()
                .vault()
                .removed_assets()
                .all(|x| removed_assets.contains(&x))
        );
        assert_eq!(
            removed_assets.len(),
            executed_transaction.account_delta().vault().removed_assets().count()
        );
    }
}

#[test]
fn executed_transaction_output_notes() {
    let executor_account = Account::mock(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE,
        Felt::ONE,
        TransactionKernel::testing_assembler(),
    );
    let account_id = executor_account.id();

    // removed assets
    let removed_asset_1 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);
    let removed_asset_2 = FungibleAsset::mock(FUNGIBLE_ASSET_AMOUNT / 2);

    let combined_asset = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT,
        )
        .expect("asset is valid"),
    );
    let removed_asset_3 = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
    let removed_asset_4 = Asset::Fungible(
        FungibleAsset::new(
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().expect("id is valid"),
            FUNGIBLE_ASSET_AMOUNT / 2,
        )
        .expect("asset is valid"),
    );

    let tag1 = NoteTag::from_account_id(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        NoteExecutionMode::Local,
    )
    .unwrap();
    let tag2 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let tag3 = NoteTag::for_public_use_case(0, 0, NoteExecutionMode::Local).unwrap();
    let aux1 = Felt::new(27);
    let aux2 = Felt::new(28);
    let aux3 = Felt::new(29);

    let note_type1 = NoteType::Private;
    let note_type2 = NoteType::Public;
    let note_type3 = NoteType::Public;

    tag1.validate(note_type1).expect("note tag 1 should support private notes");
    tag2.validate(note_type2).expect("note tag 2 should support public notes");
    tag3.validate(note_type3).expect("note tag 3 should support public notes");

    // In this test we create 3 notes. Note 1 is private, Note 2 is public and Note 3 is public
    // without assets.

    // Create the expected output note for Note 2 which is public
    let serial_num_2 = Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);
    let note_script_2 =
        NoteScript::compile(DEFAULT_NOTE_CODE, TransactionKernel::testing_assembler()).unwrap();
    let inputs_2 = NoteInputs::new(vec![ONE]).unwrap();
    let metadata_2 =
        NoteMetadata::new(account_id, note_type2, tag2, NoteExecutionHint::none(), aux2).unwrap();
    let vault_2 = NoteAssets::new(vec![removed_asset_3, removed_asset_4]).unwrap();
    let recipient_2 = NoteRecipient::new(serial_num_2, note_script_2, inputs_2);
    let expected_output_note_2 = Note::new(vault_2, metadata_2, recipient_2);

    // Create the expected output note for Note 3 which is public
    let serial_num_3 = Word::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]);
    let note_script_3 =
        NoteScript::compile(DEFAULT_NOTE_CODE, TransactionKernel::testing_assembler()).unwrap();
    let inputs_3 = NoteInputs::new(vec![ONE, Felt::new(2)]).unwrap();
    let metadata_3 = NoteMetadata::new(
        account_id,
        note_type3,
        tag3,
        NoteExecutionHint::on_block_slot(1, 2, 3),
        aux3,
    )
    .unwrap();
    let vault_3 = NoteAssets::new(vec![]).unwrap();
    let recipient_3 = NoteRecipient::new(serial_num_3, note_script_3, inputs_3);
    let expected_output_note_3 = Note::new(vault_3, metadata_3, recipient_3);

    let tx_script_src = format!(
        "\
        use.miden::contracts::wallets::basic->wallet
        use.test::account

        # Inputs:  [tag, aux, note_type, execution_hint, RECIPIENT]
        # Outputs: [note_idx]
        proc.create_note
            # pad the stack before the call to prevent accidental modification of the deeper stack
            # elements
            padw padw swapdw
            # => [tag, aux, execution_hint, note_type, RECIPIENT, pad(8)]

            call.wallet::create_note
            # => [note_idx, pad(15)]

            # remove excess PADs from the stack
            swapdw dropw dropw movdn.7 dropw drop drop drop
            # => [note_idx]
        end

        # Inputs:  [ASSET, note_idx]
        # Outputs: [ASSET, note_idx]
        proc.move_asset_to_note
            # pad the stack before call
            push.0.0.0 movdn.7 movdn.7 movdn.7 padw padw swapdw
            # => [ASSET, note_idx, pad(11)]

            call.wallet::move_asset_to_note
            # => [ASSET, note_idx, pad(11)]

            # remove excess PADs from the stack
            swapdw dropw dropw swapw movdn.7 drop drop drop
            # => [ASSET, note_idx]
        end

        ## TRANSACTION SCRIPT
        ## ========================================================================================
        begin
            ## Send some assets from the account vault
            ## ------------------------------------------------------------------------------------
            # partially deplete fungible asset balance
            push.0.1.2.3                        # recipient
            push.{EXECUTION_HINT_1}             # note execution hint
            push.{NOTETYPE1}                    # note_type
            push.{aux1}                         # aux
            push.{tag1}                         # tag
            exec.create_note
            # => [note_idx]
            
            push.{REMOVED_ASSET_1}              # asset_1
            # => [ASSET, note_idx]

            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_2}              # asset_2
            exec.move_asset_to_note dropw drop
            # => []

            # send non-fungible asset
            push.{RECIPIENT2}                   # recipient
            push.{EXECUTION_HINT_2}             # note execution hint
            push.{NOTETYPE2}                    # note_type
            push.{aux2}                         # aux
            push.{tag2}                         # tag
            exec.create_note
            # => [note_idx]

            push.{REMOVED_ASSET_3}              # asset_3
            exec.move_asset_to_note dropw
            # => [note_idx]

            push.{REMOVED_ASSET_4}              # asset_4
            exec.move_asset_to_note dropw drop
            # => []

            # create a public note without assets
            push.{RECIPIENT3}                   # recipient
            push.{EXECUTION_HINT_3}             # note execution hint
            push.{NOTETYPE3}                    # note_type
            push.{aux3}                         # aux
            push.{tag3}                         # tag
            exec.create_note drop
            # => []

            ## Update the account nonce
            ## ------------------------------------------------------------------------------------
            push.1 call.account::incr_nonce drop
            # => []
        end
    ",
        REMOVED_ASSET_1 = word_to_masm_push_string(&Word::from(removed_asset_1)),
        REMOVED_ASSET_2 = word_to_masm_push_string(&Word::from(removed_asset_2)),
        REMOVED_ASSET_3 = word_to_masm_push_string(&Word::from(removed_asset_3)),
        REMOVED_ASSET_4 = word_to_masm_push_string(&Word::from(removed_asset_4)),
        RECIPIENT2 =
            word_to_masm_push_string(&Word::from(expected_output_note_2.recipient().digest())),
        RECIPIENT3 =
            word_to_masm_push_string(&Word::from(expected_output_note_3.recipient().digest())),
        NOTETYPE1 = note_type1 as u8,
        NOTETYPE2 = note_type2 as u8,
        NOTETYPE3 = note_type3 as u8,
        EXECUTION_HINT_1 = Felt::from(NoteExecutionHint::always()),
        EXECUTION_HINT_2 = Felt::from(NoteExecutionHint::none()),
        EXECUTION_HINT_3 = Felt::from(NoteExecutionHint::on_block_slot(11, 22, 33)),
    );

    let tx_script = TransactionScript::compile(
        tx_script_src,
        [],
        TransactionKernel::testing_assembler_with_mock_account().with_debug_mode(true),
    )
    .unwrap();

    // expected delta
    // --------------------------------------------------------------------------------------------
    // execute the transaction and get the witness

    let tx_context = TransactionContextBuilder::new(executor_account)
        .with_mock_notes_preserved_with_account_vault_delta()
        .tx_script(tx_script)
        .expected_notes(vec![
            OutputNote::Full(expected_output_note_2.clone()),
            OutputNote::Full(expected_output_note_3.clone()),
        ])
        .build();

    let executed_transaction = tx_context.execute().unwrap();

    // output notes
    // --------------------------------------------------------------------------------------------
    let output_notes = executed_transaction.output_notes();

    // check the total number of notes
    // NOTE: the mock state already contains 3 output notes
    assert_eq!(output_notes.num_notes(), 6);

    // assert that the expected output note 1 is present
    let resulting_output_note_1 = executed_transaction.output_notes().get_note(3);

    let expected_recipient_1 =
        Digest::from([Felt::new(0), Felt::new(1), Felt::new(2), Felt::new(3)]);
    let expected_note_assets_1 = NoteAssets::new(vec![combined_asset]).unwrap();
    let expected_note_id_1 = NoteId::new(expected_recipient_1, expected_note_assets_1.commitment());
    assert_eq!(resulting_output_note_1.id(), expected_note_id_1);

    // assert that the expected output note 2 is present
    let resulting_output_note_2 = executed_transaction.output_notes().get_note(4);

    let expected_note_id_2 = expected_output_note_2.id();
    let expected_note_metadata_2 = expected_output_note_2.metadata();
    assert_eq!(
        NoteHeader::from(resulting_output_note_2),
        NoteHeader::new(expected_note_id_2, *expected_note_metadata_2)
    );

    // assert that the expected output note 3 is present and has no assets
    let resulting_output_note_3 = executed_transaction.output_notes().get_note(5);

    assert_eq!(expected_output_note_3.id(), resulting_output_note_3.id());
    assert_eq!(expected_output_note_3.assets(), resulting_output_note_3.assets().unwrap());

    // make sure that the number of note inputs remains the same
    let resulting_note_2_recipient =
        resulting_output_note_2.recipient().expect("output note 2 is not full");
    assert_eq!(
        resulting_note_2_recipient.inputs().num_values(),
        expected_output_note_2.inputs().num_values()
    );

    let resulting_note_3_recipient =
        resulting_output_note_3.recipient().expect("output note 3 is not full");
    assert_eq!(
        resulting_note_3_recipient.inputs().num_values(),
        expected_output_note_3.inputs().num_values()
    );
}

#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn prove_witness_and_verify() {
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();
    let source_manager = tx_context.source_manager();

    let account_id = tx_context.tx_inputs().account().id();

    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let notes = tx_context.tx_inputs().input_notes().clone();
    let tx_args = tx_context.tx_args().clone();
    let executor = TransactionExecutor::new(Arc::new(tx_context), None);
    let executed_transaction = executor
        .execute_transaction(account_id, block_ref, notes, tx_args, Arc::clone(&source_manager))
        .unwrap();
    let executed_transaction_id = executed_transaction.id();

    let proof_options = ProvingOptions::default();
    let prover = LocalTransactionProver::new(proof_options);
    let proven_transaction = prover.prove(executed_transaction.into()).unwrap();

    assert_eq!(proven_transaction.id(), executed_transaction_id);

    let serialized_transaction = proven_transaction.to_bytes();
    let proven_transaction = ProvenTransaction::read_from_bytes(&serialized_transaction).unwrap();
    let verifier = TransactionVerifier::new(MIN_PROOF_SECURITY_LEVEL);
    assert!(verifier.verify(&proven_transaction).is_ok());
}

// TEST TRANSACTION SCRIPT
// ================================================================================================

#[test]
fn test_tx_script_inputs() {
    let tx_script_input_key = [Felt::new(9999), Felt::new(8888), Felt::new(9999), Felt::new(8888)];
    let tx_script_input_value = [Felt::new(9), Felt::new(8), Felt::new(7), Felt::new(6)];
    let tx_script_src = format!(
        "
        begin
            # push the tx script input key onto the stack
            push.{key}

            # load the tx script input value from the map and read it onto the stack
            adv.push_mapval adv_loadw

            # assert that the value is correct
            push.{value} assert_eqw
        end
        ",
        key = word_to_masm_push_string(&tx_script_input_key),
        value = word_to_masm_push_string(&tx_script_input_value)
    );

    let tx_script = TransactionScript::compile(
        tx_script_src,
        [(tx_script_input_key, tx_script_input_value.into())],
        TransactionKernel::testing_assembler(),
    )
    .unwrap();

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .tx_script(tx_script)
        .build();

    let executed_transaction = tx_context.execute();

    assert!(
        executed_transaction.is_ok(),
        "Transaction execution failed {:?}",
        executed_transaction,
    );
}

#[test]
fn test_tx_script_args() -> anyhow::Result<()> {
    let tx_script_src = r#"
        begin
            # => [TX_SCRIPT_ARGS_KEY]
            # `TX_SCRIPT_ARGS_KEY` value, which is located on the stack at the beginning of 
            # the execution, is the advice map key which allows to obtain transaction script args 
            # which were specified during the `TransactionScript` creation (see below). In this
            # example transaction args is an array of Felts from 1 to 7.

            # move the transaction args from advice map to the advice stack
            adv.push_mapval
            # OS => [TX_SCRIPT_ARGS_KEY]
            # AS => [7, 6, 5, 4, 3, 2, 1]

            # drop the args commitment
            dropw
            # OS => []
            # AS => [7, 6, 5, 4, 3, 2, 1]

            # Move the transaction arguments array from advice stack to the operand stack. It 
            # consists of 7 Felts, so we could use `adv_push.7` instruction to load them all at once
            adv_push.7
            # OS => [7, 6, 5, 4, 3, 2, 1]
            # AS => []

            # assert that the transaction arguments array consists of correct values
            push.4.5.6.7
            assert_eqw.err="last four values in the transaction args array are incorrect"
            # OS => [3, 2, 1]
            # AS => []
            
            # To use more convenient `assert_eqw` instruction we should push `0` first, but notice 
            # that there are only three values from the original array left on the stack. We could 
            # do so because there are no other elements on the stack left except for the argument 
            # values (hidden values deeper on the stack are zeros). In case there are some values 
            # deeper on the stack, we should assert values one by one using `push.n assert_eq`.
            push.0.1.2.3
            assert_eqw.err="first three values in the transaction args array are incorrect"
        end"#;

    let tx_script =
        TransactionScript::compile(tx_script_src, [], TransactionKernel::testing_assembler())
            .context("failed to compile transaction script")?
            .with_args(&[
                ONE,
                Felt::new(2),
                Felt::new(3),
                Felt::new(4),
                Felt::new(5),
                Felt::new(6),
                Felt::new(7),
            ])?;

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .tx_script(tx_script)
        .build();

    tx_context.execute().context("failed to execute transaction")?;

    Ok(())
}

#[test]
fn test_tx_script_args_collision() -> anyhow::Result<()> {
    let collision_elements = vec![ONE, Felt::new(2), Felt::new(3), Felt::new(4)];
    let collision_key = Hasher::hash_elements(&collision_elements);

    let script_args_collision_err = TransactionScript::compile(
        "begin nop end",
        [(*collision_key, vec![ONE, Felt::new(2)])],
        TransactionKernel::testing_assembler(),
    )
    .context("failed to compile transaction script")?
    .with_args(&collision_elements)
    .unwrap_err();

    assert_matches!(script_args_collision_err, TransactionScriptError::ScriptArgsCollision {
        key,
        new_value,
        old_value,
    } => {
        assert_eq!(key, collision_key);
        assert_eq!(new_value, collision_elements);
        assert_eq!(old_value, [ONE, Felt::new(2)]);
    });

    Ok(())
}

/// Tests that an account can call code in a custom library when loading that library into the
/// executor.
///
/// The call chain and dependency graph in this test is:
/// `tx script -> account code -> external library`
#[test]
fn transaction_executor_account_code_using_custom_library() {
    const EXTERNAL_LIBRARY_CODE: &str = r#"
      use.miden::account

      export.incr_nonce_by_four
        dup eq.4 assert.err="nonce increment is not 4"
        exec.account::incr_nonce
      end"#;

    const ACCOUNT_COMPONENT_CODE: &str = "
      use.external_library::external_module

      export.custom_nonce_incr
        push.4 exec.external_module::incr_nonce_by_four
      end";

    let source_manager = Arc::new(DefaultSourceManager::default());
    let external_library_module = Module::parser(ModuleKind::Library)
        .parse_str(
            LibraryPath::new("external_library::external_module").unwrap(),
            EXTERNAL_LIBRARY_CODE,
            &source_manager,
        )
        .unwrap();
    let external_library = TransactionKernel::assembler()
        .assemble_library([external_library_module])
        .unwrap();

    let mut assembler = TransactionKernel::assembler();
    assembler.add_vendored_library(&external_library).unwrap();

    let account_component_module = Module::parser(ModuleKind::Library)
        .parse_str(
            LibraryPath::new("account_component::account_module").unwrap(),
            ACCOUNT_COMPONENT_CODE,
            &source_manager,
        )
        .unwrap();

    let account_component_lib =
        assembler.clone().assemble_library([account_component_module]).unwrap();

    let tx_script_src = "\
          use.account_component::account_module

          begin
            call.account_module::custom_nonce_incr
          end";

    let account_component =
        AccountComponent::new(account_component_lib.clone(), vec![StorageSlot::empty_value()])
            .unwrap()
            .with_supports_all_types();

    // Build an existing account with nonce 1.
    let native_account = AccountBuilder::new(ChaCha20Rng::from_os_rng().random())
        .with_component(account_component)
        .build_existing()
        .unwrap();

    let tx_script = TransactionScript::compile(
        tx_script_src,
        [],
        // Add the account component library since the transaction script is calling the account's
        // procedure.
        assembler.with_library(&account_component_lib).unwrap(),
    )
    .unwrap();

    let tx_context = TransactionContextBuilder::new(native_account.clone())
        .tx_script(tx_script)
        .build();

    let executed_tx = tx_context.execute().unwrap();

    // Account's initial nonce of 1 should have been incremented by 4.
    assert_eq!(executed_tx.account_delta().nonce().unwrap(), Felt::new(5));
}

#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn test_execute_program() {
    let test_module_source = "
        export.foo
            push.3.4
            add
            swapw dropw
        end
    ";

    let assembler = TransactionKernel::assembler();
    let source_manager = assembler.source_manager();
    let test_module = Module::parser(assembly::ast::ModuleKind::Library)
        .parse_str(
            LibraryPath::new("test::module_1").unwrap(),
            test_module_source,
            &assembler.source_manager(),
        )
        .unwrap();
    let assembler = assembler.with_module(test_module).unwrap();

    let source = "
    use.test::module_1
    use.std::sys
    
    begin
        push.1.2
        call.module_1::foo
        exec.sys::truncate_stack
    end
    ";

    let tx_script = TransactionScript::compile(source, [], assembler)
        .expect("failed to compile the source script");

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .tx_script(tx_script.clone())
        .build();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let advice_inputs = tx_context.tx_args().advice_inputs().clone();

    let executor = TransactionExecutor::new(Arc::new(tx_context), None);

    let stack_outputs = executor
        .execute_tx_view_script(
            account_id,
            block_ref,
            tx_script,
            advice_inputs,
            Vec::default(),
            source_manager,
        )
        .unwrap();

    assert_eq!(stack_outputs[..3], [Felt::new(7), Felt::new(2), ONE]);
}

#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn test_check_note_consumability() {
    // Success (well known notes)
    // --------------------------------------------------------------------------------------------
    let p2id_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let p2idr_note = create_p2idr_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_UPDATABLE_CODE.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        BlockNumber::default(),
        &mut RpoRandomCoin::new([ONE, Felt::new(2), Felt::new(3), Felt::new(4)]),
    )
    .unwrap();

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .input_notes(vec![p2id_note, p2idr_note])
        .build();
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let target_account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor: TransactionExecutor =
        TransactionExecutor::new(Arc::new(tx_context), None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(
            target_account_id,
            block_ref,
            input_notes,
            tx_args,
            source_manager,
        )
        .unwrap();
    assert_matches!(execution_check_result, NoteAccountExecution::Success);

    // Success (custom notes)
    // --------------------------------------------------------------------------------------------
    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .build();
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor: TransactionExecutor =
        TransactionExecutor::new(Arc::new(tx_context), None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(account_id, block_ref, input_notes, tx_args, source_manager)
        .unwrap();
    assert_matches!(execution_check_result, NoteAccountExecution::Success);

    // Failure
    // --------------------------------------------------------------------------------------------
    let sender = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let failing_note_1 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.1 drop push.0 div end")
    .build(&TransactionKernel::testing_assembler())
    .unwrap();

    let failing_note_2 = NoteBuilder::new(
        sender,
        ChaCha20Rng::from_seed(ChaCha20Rng::from_seed([0_u8; 32]).random()),
    )
    .code("begin push.2 drop push.0 div end")
    .build(&TransactionKernel::testing_assembler())
    .unwrap();

    let tx_context = TransactionContextBuilder::with_standard_account(ONE)
        .with_mock_notes_preserved()
        .input_notes(vec![failing_note_1, failing_note_2.clone()])
        .build();
    let source_manager = tx_context.source_manager();

    let input_notes = tx_context.input_notes().clone();
    let input_note_ids =
        input_notes.iter().map(|input_note| input_note.id()).collect::<Vec<NoteId>>();
    let account_id = tx_context.account().id();
    let block_ref = tx_context.tx_inputs().block_header().block_num();
    let tx_args = tx_context.tx_args().clone();

    let executor: TransactionExecutor =
        TransactionExecutor::new(Arc::new(tx_context), None).with_tracing();
    let notes_checker = NoteConsumptionChecker::new(&executor);

    let execution_check_result = notes_checker
        .check_notes_consumability(account_id, block_ref, input_notes, tx_args, source_manager)
        .unwrap();

    assert_matches!(execution_check_result, NoteAccountExecution::Failure {
        failed_note_id,
        successful_notes,
        error: Some(e)} => {
            assert_eq!(failed_note_id, failing_note_2.id());
            assert_eq!(successful_notes, input_note_ids[..2].to_vec());
            assert_matches!(e, TransactionExecutorError::TransactionProgramExecutionFailed(
              ExecutionError::DivideByZero { .. }
            ));
        }
    );
}
