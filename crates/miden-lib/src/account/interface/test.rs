use alloc::{string::ToString, vec::Vec};

use assert_matches::assert_matches;
use miden_objects::{
    AccountError, Felt, NoteError, Word, ZERO,
    account::{AccountBuilder, AccountComponent, AccountType, StorageSlot},
    assembly::{Assembler, diagnostics::NamedSource},
    asset::{FungibleAsset, NonFungibleAsset, TokenSymbol},
    crypto::{
        dsa::rpo_falcon512::PublicKey,
        rand::{FeltRng, RpoRandomCoin},
    },
    note::{
        Note, NoteAssets, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient, NoteScript,
        NoteTag, NoteType,
    },
    testing::account_id::{
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE,
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2,
    },
};

use crate::{
    account::{
        auth::AuthRpoFalcon512,
        faucets::BasicFungibleFaucet,
        interface::{AccountInterface, NoteAccountCompatibility},
        wallets::BasicWallet,
    },
    note::{create_p2id_note, create_p2ide_note, create_swap_note},
    transaction::TransactionKernel,
};

// DEFAULT NOTES
// ================================================================================================

#[test]
fn test_basic_wallet_default_notes() {
    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let wallet_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(BasicWallet)
        .with_assets(vec![FungibleAsset::mock(20)])
        .build_existing()
        .expect("failed to create wallet account");

    let wallet_account_interface = AccountInterface::from(&wallet_account);

    let mock_seed = Word::from([Felt::new(4), Felt::new(5), Felt::new(6), Felt::new(7)]).as_bytes();
    let faucet_account = AccountBuilder::new(mock_seed)
        .account_type(AccountType::FungibleFaucet)
        .with_auth_component(get_mock_auth_component())
        .with_component(
            BasicFungibleFaucet::new(
                TokenSymbol::new("POL").expect("invalid token symbol"),
                10,
                Felt::new(100),
            )
            .expect("failed to create a fungible faucet component"),
        )
        .build_existing()
        .expect("failed to create wallet account");
    let faucet_account_interface = AccountInterface::from(&faucet_account);

    let p2id_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    let p2ide_note = create_p2ide_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        None,
        None,
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    let offered_asset = NonFungibleAsset::mock(&[5, 6, 7, 8]);
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let (swap_note, _) = create_swap_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        offered_asset,
        requested_asset,
        NoteType::Public,
        ZERO,
        NoteType::Public,
        ZERO,
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    // Basic wallet
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        wallet_account_interface.is_compatible_with(&p2id_note)
    );
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        wallet_account_interface.is_compatible_with(&p2ide_note)
    );
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        wallet_account_interface.is_compatible_with(&swap_note)
    );

    // Basic fungible faucet
    assert_eq!(
        NoteAccountCompatibility::No,
        faucet_account_interface.is_compatible_with(&p2id_note)
    );
    assert_eq!(
        NoteAccountCompatibility::No,
        faucet_account_interface.is_compatible_with(&p2ide_note)
    );
    assert_eq!(
        NoteAccountCompatibility::No,
        faucet_account_interface.is_compatible_with(&swap_note)
    );
}

/// Checks the compatibility of the basic notes (P2ID, P2IDE and SWAP) against an account with a
/// custom interface containing a procedure from the basic wallet.
///
/// In that setup check against P2ID and P2IDE notes should result in `Maybe`, and the check against
/// SWAP should result in `No`.
#[test]
fn test_custom_account_default_note() {
    let account_custom_code_source = "
        use.miden::contracts::wallets::basic

        export.basic::receive_asset
    ";

    let account_component = AccountComponent::compile(
        account_custom_code_source,
        TransactionKernel::testing_assembler(),
        vec![],
    )
    .unwrap()
    .with_supports_all_types();

    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let target_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(account_component.clone())
        .build_existing()
        .unwrap();
    let target_account_interface = AccountInterface::from(&target_account);

    let p2id_note = create_p2id_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    let p2ide_note = create_p2ide_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap(),
        vec![FungibleAsset::mock(10)],
        None,
        None,
        NoteType::Public,
        Default::default(),
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    let offered_asset = NonFungibleAsset::mock(&[5, 6, 7, 8]);
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let (swap_note, _) = create_swap_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        offered_asset,
        requested_asset,
        NoteType::Public,
        ZERO,
        NoteType::Public,
        ZERO,
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    )
    .unwrap();

    assert_eq!(
        NoteAccountCompatibility::Maybe,
        target_account_interface.is_compatible_with(&p2id_note)
    );
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        target_account_interface.is_compatible_with(&p2ide_note)
    );
    assert_eq!(
        NoteAccountCompatibility::No,
        target_account_interface.is_compatible_with(&swap_note)
    );
}

/// Checks the function `create_swap_note` should fail if the requested asset is the same as the
/// offered asset.
#[test]
fn test_required_asset_same_as_offered() {
    let offered_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);
    let requested_asset = NonFungibleAsset::mock(&[1, 2, 3, 4]);

    let result = create_swap_note(
        ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE.try_into().unwrap(),
        offered_asset,
        requested_asset,
        NoteType::Public,
        ZERO,
        NoteType::Public,
        ZERO,
        &mut RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])),
    );

    assert_matches!(result, Err(NoteError::Other { error_msg, .. }) if error_msg == "requested asset same as offered asset".into());
}

// CUSTOM NOTES
// ================================================================================================

#[test]
fn test_basic_wallet_custom_notes() {
    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let wallet_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(BasicWallet)
        .with_assets(vec![FungibleAsset::mock(20)])
        .build_existing()
        .expect("failed to create wallet account");
    let wallet_account_interface = AccountInterface::from(&wallet_account);

    let sender_account_id = ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap();
    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let tag = NoteTag::from_account_id(wallet_account.id());
    let metadata = NoteMetadata::new(
        sender_account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .unwrap();
    let vault = NoteAssets::new(vec![FungibleAsset::mock(100)]).unwrap();

    let compatible_source_code = "
        use.miden::tx
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note

                # unsupported procs
                call.fungible_faucet::distribute
                call.fungible_faucet::burn
            else
                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
            end
        end
    ";
    let note_script =
        NoteScript::compile(compatible_source_code, TransactionKernel::testing_assembler())
            .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let compatible_custom_note = Note::new(vault.clone(), metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        wallet_account_interface.is_compatible_with(&compatible_custom_note)
    );

    let incompatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # unsupported procs
                call.fungible_faucet::distribute
                call.fungible_faucet::burn
            else
                # unsupported proc
                call.fungible_faucet::distribute

                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
            end
        end
    ";
    let note_script =
        NoteScript::compile(incompatible_source_code, TransactionKernel::testing_assembler())
            .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let incompatible_custom_note = Note::new(vault, metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::No,
        wallet_account_interface.is_compatible_with(&incompatible_custom_note)
    );
}

#[test]
fn test_basic_fungible_faucet_custom_notes() {
    let mock_seed = Word::from([Felt::new(4), Felt::new(5), Felt::new(6), Felt::new(7)]).as_bytes();
    let faucet_account = AccountBuilder::new(mock_seed)
        .account_type(AccountType::FungibleFaucet)
        .with_auth_component(get_mock_auth_component())
        .with_component(
            BasicFungibleFaucet::new(
                TokenSymbol::new("POL").expect("invalid token symbol"),
                10,
                Felt::new(100),
            )
            .expect("failed to create a fungible faucet component"),
        )
        .build_existing()
        .expect("failed to create wallet account");
    let faucet_account_interface = AccountInterface::from(&faucet_account);

    let sender_account_id = ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE_2.try_into().unwrap();
    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let tag = NoteTag::from_account_id(faucet_account.id());
    let metadata = NoteMetadata::new(
        sender_account_id,
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .unwrap();
    let vault = NoteAssets::new(vec![FungibleAsset::mock(100)]).unwrap();

    let compatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # supported procs
                call.fungible_faucet::distribute
                call.fungible_faucet::burn
            else
                # supported proc
                call.fungible_faucet::distribute

                # unsupported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
            end
        end
    ";
    let note_script =
        NoteScript::compile(compatible_source_code, TransactionKernel::testing_assembler())
            .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let compatible_custom_note = Note::new(vault.clone(), metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        faucet_account_interface.is_compatible_with(&compatible_custom_note)
    );

    let incompatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # supported procs
                call.fungible_faucet::distribute
                call.fungible_faucet::burn

                # unsupported proc
                call.wallet::receive_asset
            else
                # supported proc
                call.fungible_faucet::burn

                # unsupported procs
                call.wallet::move_asset_to_note
            end
        end
    ";
    let note_script =
        NoteScript::compile(incompatible_source_code, TransactionKernel::testing_assembler())
            .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let incompatible_custom_note = Note::new(vault, metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::No,
        faucet_account_interface.is_compatible_with(&incompatible_custom_note)
    );
}

/// Checks the compatibility of the note with custom code against an account with one custom
/// interface.
///
/// In that setup the note script should have at least one execution branch with procedures from the
/// account interface for being `Maybe` compatible.
#[test]
fn test_custom_account_custom_notes() {
    let account_custom_code_source = "
        export.procedure_1
            push.1.2.3.4 dropw
        end

        export.procedure_2
            push.5.6.7.8 dropw
        end
    ";

    let account_component = AccountComponent::compile_with_path(
        account_custom_code_source,
        TransactionKernel::testing_assembler(),
        vec![],
        "test::account::component_1",
    )
    .unwrap()
    .with_supports_all_types();

    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let target_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(account_component.clone())
        .build_existing()
        .unwrap();
    let target_account_interface = AccountInterface::from(&target_account);

    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let sender_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(BasicWallet)
        .with_assets(vec![FungibleAsset::mock(20)])
        .build_existing()
        .expect("failed to create wallet account");

    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let tag = NoteTag::from_account_id(target_account.id());
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .unwrap();
    let vault = NoteAssets::new(vec![FungibleAsset::mock(100)]).unwrap();

    let compatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.test::account::component_1->test_account

        begin
            push.1
            if.true
                # supported proc
                call.test_account::procedure_1

                # unsupported proc
                call.wallet::receive_asset
            else
                # supported procs
                call.test_account::procedure_1
                call.test_account::procedure_2
            end
        end
    ";
    let note_script = NoteScript::compile(
        compatible_source_code,
        TransactionKernel::testing_assembler()
            .with_dynamic_library(account_component.library())
            .unwrap(),
    )
    .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let compatible_custom_note = Note::new(vault.clone(), metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        target_account_interface.is_compatible_with(&compatible_custom_note)
    );

    let incompatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.test::account::component_1->test_account

        begin
            push.1
            if.true
                call.wallet::receive_asset
                call.test_account::procedure_1
            else
                call.test_account::procedure_2
                call.wallet::move_asset_to_note
            end
        end
    ";
    let note_script = NoteScript::compile(
        incompatible_source_code,
        TransactionKernel::testing_assembler()
            .with_dynamic_library(account_component.library())
            .unwrap(),
    )
    .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let incompatible_custom_note = Note::new(vault, metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::No,
        target_account_interface.is_compatible_with(&incompatible_custom_note)
    );
}

/// Checks the compatibility of the note with custom code against an account with many custom
/// interfaces.
///
/// In that setup the note script should have at least one execution branch with procedures from the
/// account interface for being `Maybe` compatible.
#[test]
fn test_custom_account_multiple_components_custom_notes() {
    let account_custom_code_source = "
        export.procedure_1
            push.1.2.3.4 dropw
        end

        export.procedure_2
            push.5.6.7.8 dropw
        end
    ";

    let custom_component = AccountComponent::compile_with_path(
        account_custom_code_source,
        TransactionKernel::testing_assembler(),
        vec![],
        "test::account::component_1",
    )
    .unwrap()
    .with_supports_all_types();

    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let target_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(custom_component.clone())
        .with_component(BasicWallet)
        .build_existing()
        .unwrap();
    let target_account_interface = AccountInterface::from(&target_account);

    let mock_seed = Word::from([0, 1, 2, 3u32]).as_bytes();
    let sender_account = AccountBuilder::new(mock_seed)
        .with_auth_component(get_mock_auth_component())
        .with_component(BasicWallet)
        .with_assets(vec![FungibleAsset::mock(20)])
        .build_existing()
        .expect("failed to create wallet account");

    let serial_num = RpoRandomCoin::new(Word::from([1, 2, 3, 4u32])).draw_word();
    let tag = NoteTag::from_account_id(target_account.id());
    let metadata = NoteMetadata::new(
        sender_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::always(),
        Default::default(),
    )
    .unwrap();
    let vault = NoteAssets::new(vec![FungibleAsset::mock(100)]).unwrap();

    let compatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::auth::basic->basic_auth
        use.test::account::component_1->test_account
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
                call.test_account::procedure_1
                call.test_account::procedure_2
            else
                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
                call.test_account::procedure_1
                call.test_account::procedure_2

                # unsupported proc
                call.fungible_faucet::distribute
            end
        end
    ";
    let note_script = NoteScript::compile(
        compatible_source_code,
        TransactionKernel::testing_assembler()
            .with_dynamic_library(custom_component.library())
            .unwrap(),
    )
    .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let compatible_custom_note = Note::new(vault.clone(), metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::Maybe,
        target_account_interface.is_compatible_with(&compatible_custom_note)
    );

    let incompatible_source_code = "
        use.miden::contracts::wallets::basic->wallet
        use.miden::contracts::auth::basic->basic_auth
        use.test::account::component_1->test_account
        use.miden::contracts::faucets::basic_fungible->fungible_faucet

        begin
            push.1
            if.true
                # supported procs
                call.wallet::receive_asset
                call.wallet::move_asset_to_note
                call.test_account::procedure_1
                call.test_account::procedure_2

                # unsupported proc
                call.fungible_faucet::distribute
            else
                # supported procs
                call.test_account::procedure_1
                call.test_account::procedure_2

                # unsupported proc
                call.fungible_faucet::burn
            end
        end
    ";
    let note_script = NoteScript::compile(
        incompatible_source_code,
        TransactionKernel::testing_assembler()
            .with_dynamic_library(custom_component.library())
            .unwrap(),
    )
    .unwrap();
    let recipient = NoteRecipient::new(serial_num, note_script, NoteInputs::default());
    let incompatible_custom_note = Note::new(vault.clone(), metadata, recipient);
    assert_eq!(
        NoteAccountCompatibility::No,
        target_account_interface.is_compatible_with(&incompatible_custom_note)
    );
}

// HELPER TRAIT
// ================================================================================================

/// [AccountComponentExt] is a helper trait which only implements the `compile_with_path` procedure
/// for testing purposes.
trait AccountComponentExt {
    fn compile_with_path(
        source_code: impl ToString,
        assembler: Assembler,
        storage_slots: Vec<StorageSlot>,
        library_path: impl AsRef<str>,
    ) -> Result<AccountComponent, AccountError>;
}

impl AccountComponentExt for AccountComponent {
    /// Returns a new [`AccountComponent`] whose library is compiled from the provided `source_code`
    /// using the specified `assembler`, `library_path`, and with the given `storage_slots`.
    ///
    /// All procedures exported from the provided code will become members of the account's public
    /// interface when added to an [`AccountCode`](crate::account::AccountCode), and could be called
    /// using the provided library path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the compilation of the provided source code fails.
    /// - The number of storage slots exceeds 255.
    fn compile_with_path(
        source_code: impl ToString,
        assembler: Assembler,
        storage_slots: Vec<StorageSlot>,
        library_path: impl AsRef<str>,
    ) -> Result<Self, AccountError> {
        let source = NamedSource::new(library_path, source_code.to_string());
        let library = assembler
            .assemble_library([source])
            .map_err(AccountError::AccountComponentAssemblyError)?;

        Self::new(library, storage_slots)
    }
}

fn get_mock_auth_component() -> AuthRpoFalcon512 {
    let mock_public_key = PublicKey::new(Word::from([0, 1, 2, 3u32]));
    AuthRpoFalcon512::new(mock_public_key)
}
