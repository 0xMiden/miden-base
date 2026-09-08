use assert_matches::assert_matches;
use miden_protocol::account::auth::{AuthScheme, AuthSecretKey};
use miden_protocol::account::{Account, AccountId};
use miden_protocol::asset::{Asset, FungibleAsset};
use miden_protocol::errors::MasmError;
use miden_protocol::errors::tx_kernel::ERR_EPILOGUE_EXECUTED_TRANSACTION_IS_EMPTY;
use miden_protocol::note::{Note, NoteType};
use miden_protocol::testing::account_id::ACCOUNT_ID_SENDER;
use miden_protocol::{Felt, Word};
use miden_standards::errors::standards::{
    ERR_AUTH_PASS_THROUGH_ACCOUNT_CREATED_WITH_ASSETS,
    ERR_AUTH_PASS_THROUGH_ACCOUNT_STATE_CHANGED,
};
use miden_standards::tx_script::PassThroughSingleP2idTransactionScript;
use miden_testing::{AccountState, Auth, MockChain, assert_transaction_executor_error};
use miden_tx::TransactionExecutorError;
use miden_tx::auth::BasicAuthenticator;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::scripts::pass_through::{
    AUTH_SCHEME,
    add_pass_through_account,
    add_pass_through_account_with,
};

// CONSTANTS
// ================================================================================================

const SERIAL_NUMBER: Word = Word::new([Felt::new_unchecked(9); 4]);

/// The verification base fee the fee-charging chain in this module is built with. Any non-zero
/// value works.
const VERIFICATION_BASE_FEE: u32 = 500;

// HELPERS
// ================================================================================================

/// The asset the fee note in these tests carries.
fn fee_asset() -> Asset {
    FungibleAsset::mock(10)
}

/// Sets up the common shape of a pass-through transaction: the immutable account, a target wallet
/// to forward to, and one TX_FEE note to forward.
fn pass_through_setup() -> anyhow::Result<(Account, AccountId, Note, MockChain)> {
    let mut builder = MockChain::builder();
    let account = add_pass_through_account(&mut builder)?;
    let target = builder.add_existing_wallet(Auth::BasicAuth {
        auth_scheme: AuthScheme::Falcon512Poseidon2,
    })?;
    let fee_note = builder.add_tx_fee_note(ACCOUNT_ID_SENDER.try_into()?, &[fee_asset()])?;

    Ok((account, target.id(), fee_note, builder.build()?))
}

/// The pass-through script forwarding [`fee_asset`] out of `account` into a P2ID note for `target`.
fn single_p2id_script(
    account: &Account,
    target: AccountId,
) -> anyhow::Result<PassThroughSingleP2idTransactionScript> {
    Ok(PassThroughSingleP2idTransactionScript::new(
        &account.code_interface(),
        target,
        NoteType::Public,
        SERIAL_NUMBER,
        [fee_asset().id()],
    )?)
}

// TESTS
// ================================================================================================

/// A transaction that leaves the account holding what an input note deposited is rejected: the
/// commitment changed.
#[tokio::test]
async fn pass_through_auth_rejects_a_state_change() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = add_pass_through_account(&mut builder)?;

    // a plain P2ID note deposits into the account, and nothing moves the assets back out
    let note = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[FungibleAsset::mock(10)],
        NoteType::Public,
    )?;
    let mock_chain = builder.build()?;

    let result = mock_chain
        .build_transaction(account.id())
        .authenticated_input_note(note.id())
        .build()?
        .execute()
        .await;

    assert_transaction_executor_error!(result, ERR_AUTH_PASS_THROUGH_ACCOUNT_STATE_CHANGED);

    Ok(())
}

/// On a fee-charging chain the auth procedure creates no TX_FEE note, so the transaction's only
/// output is the P2ID note the script created.
#[tokio::test]
async fn pass_through_auth_creates_no_fee_note_on_a_fee_charging_chain() -> anyhow::Result<()> {
    let mut builder = MockChain::builder().verification_base_fee(VERIFICATION_BASE_FEE);
    let account = add_pass_through_account(&mut builder)?;
    let target = builder.add_existing_wallet(Auth::BasicAuth {
        auth_scheme: AuthScheme::Falcon512Poseidon2,
    })?;

    let fee_asset = FungibleAsset::mock(10);
    let fee_note = builder.add_tx_fee_note(ACCOUNT_ID_SENDER.try_into()?, &[fee_asset])?;
    let mock_chain = builder.build()?;

    let script = PassThroughSingleP2idTransactionScript::new(
        &account.code_interface(),
        target.id(),
        NoteType::Public,
        SERIAL_NUMBER,
        [fee_asset.id()],
    )?;

    let executed = mock_chain
        .build_transaction(account.id())
        .authenticated_input_note(fee_note.id())
        .pass_through_single_p2id_script(&script)
        .build()?
        .execute()
        .await?;

    assert_eq!(
        executed.output_notes().num_notes(),
        1,
        "a pass-through transaction pays no fee, so the P2ID note is its only output",
    );
    assert_eq!(
        executed.output_notes().get_note(0).recipient_digest(),
        script.output_note_recipient().digest(),
    );
    assert_eq!(executed.final_account().to_commitment(), account.to_commitment());

    Ok(())
}

/// The account's key holder can deploy it themselves: the creating transaction is the one case in
/// which the nonce is incremented.
#[tokio::test]
async fn pass_through_auth_can_create_an_account() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = add_pass_through_account_with(&mut builder, [45; 32], [], AccountState::New)?;
    let mock_chain = builder.build()?;

    // a new account is passed by value, since the chain does not yet know it
    let executed = mock_chain.build_transaction(account.clone()).build()?.execute().await?;

    assert_eq!(
        executed.final_account().nonce(),
        Felt::new_unchecked(1),
        "the creating transaction is the only one that may increment the nonce",
    );

    Ok(())
}

/// Without the key nothing can be executed against the account.
#[tokio::test]
async fn pass_through_auth_requires_a_signature() -> anyhow::Result<()> {
    let (account, target, fee_note, mock_chain) = pass_through_setup()?;
    let script = single_p2id_script(&account, target)?;

    // an otherwise valid pass-through transaction, so that only the missing key can fail it
    let result = mock_chain
        .build_transaction(account.id())
        .authenticated_input_note(fee_note.id())
        .pass_through_single_p2id_script(&script)
        .authenticator(None)
        .build()?
        .execute()
        .await;

    assert_matches!(result, Err(TransactionExecutorError::MissingAuthenticator));

    Ok(())
}

/// A signature from a key other than the account's is rejected, so holding *a* key is not enough.
#[tokio::test]
async fn pass_through_auth_rejects_a_foreign_key_signature() -> anyhow::Result<()> {
    let (account, target, fee_note, mock_chain) = pass_through_setup()?;
    let script = single_p2id_script(&account, target)?;

    // re-derive the account's public key from the seed `Auth::PassThrough` uses, then bind a
    // foreign secret key to it, so the procedure gets a signature that must fail to verify
    let mut account_rng = ChaCha20Rng::from_seed(Default::default());
    let account_pub_key =
        AuthSecretKey::with_scheme_and_rng(AUTH_SCHEME, &mut account_rng)?.public_key();

    let mut foreign_rng = ChaCha20Rng::from_seed([1u8; 32]);
    let foreign_sec_key = AuthSecretKey::with_scheme_and_rng(AUTH_SCHEME, &mut foreign_rng)?;

    let authenticator = BasicAuthenticator::from_key_pairs(&[(foreign_sec_key, account_pub_key)]);

    let result = mock_chain
        .build_transaction(account.id())
        .authenticated_input_note(fee_note.id())
        .pass_through_single_p2id_script(&script)
        .authenticator(Some(authenticator))
        .build()?
        .execute()
        .await;

    assert_transaction_executor_error!(
        result,
        MasmError::from_static_str("invalid public key commitment")
    );

    Ok(())
}

/// Two successive pass-through transactions leave the account byte-identical, nonce included,
/// which is what lets batch builders build them concurrently.
#[tokio::test]
async fn pass_through_auth_leaves_the_account_untouched_across_transactions() -> anyhow::Result<()>
{
    let mut builder = MockChain::builder();
    let account = add_pass_through_account(&mut builder)?;
    let target = builder.add_existing_wallet(Auth::BasicAuth {
        auth_scheme: AuthScheme::Falcon512Poseidon2,
    })?;

    let fee_asset = FungibleAsset::mock(10);
    let first_note = builder.add_tx_fee_note(ACCOUNT_ID_SENDER.try_into()?, &[fee_asset])?;
    let second_note = builder.add_tx_fee_note(ACCOUNT_ID_SENDER.try_into()?, &[fee_asset])?;
    let mock_chain = builder.build()?;

    let script = PassThroughSingleP2idTransactionScript::new(
        &account.code_interface(),
        target.id(),
        NoteType::Public,
        SERIAL_NUMBER,
        [fee_asset.id()],
    )?;

    for note in [first_note, second_note] {
        let executed = mock_chain
            .build_transaction(account.id())
            .authenticated_input_note(note.id())
            .pass_through_single_p2id_script(&script)
            .build()?
            .execute()
            .await?;

        assert_eq!(executed.final_account().to_commitment(), account.to_commitment());
        assert_eq!(executed.final_account().nonce(), account.nonce());
    }

    Ok(())
}

/// A transaction consuming no input notes (the one shape the signature's input notes cannot bind)
/// cannot be executed against an existing pass-through account: its account patch is empty too,
/// and the kernel rejects a transaction that neither changes the account nor consumes a note.
#[tokio::test]
async fn pass_through_auth_rejects_a_transaction_without_input_notes() -> anyhow::Result<()> {
    let (account, target, _fee_note, mock_chain) = pass_through_setup()?;
    let script = single_p2id_script(&account, target)?;

    let result = mock_chain
        .build_transaction(account.id())
        .pass_through_single_p2id_script(&script)
        .build()?
        .execute()
        .await;

    assert_transaction_executor_error!(result, ERR_EPILOGUE_EXECUTED_TRANSACTION_IS_EMPTY);

    Ok(())
}

/// The creating transaction is signed like any other: skipping the state check does not skip the
/// signature.
#[tokio::test]
async fn pass_through_auth_requires_a_signature_to_create_an_account() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = add_pass_through_account_with(&mut builder, [48; 32], [], AccountState::New)?;
    let mock_chain = builder.build()?;

    let result = mock_chain
        .build_transaction(account.clone())
        .authenticator(None)
        .build()?
        .execute()
        .await;

    assert_matches!(result, Err(TransactionExecutorError::MissingAuthenticator));

    Ok(())
}

/// An account created holding assets could never move them out again (every later transaction has
/// to leave it unchanged), so the creating transaction is rejected instead.
#[tokio::test]
async fn pass_through_auth_rejects_an_account_created_holding_assets() -> anyhow::Result<()> {
    let mut builder = MockChain::builder();
    let account = add_pass_through_account_with(&mut builder, [49; 32], [], AccountState::New)?;

    // a plain P2ID note deposits into the account while it is being created
    let note = builder.add_p2id_note(
        ACCOUNT_ID_SENDER.try_into()?,
        account.id(),
        &[fee_asset()],
        NoteType::Public,
    )?;
    let mock_chain = builder.build()?;

    let result = mock_chain
        .build_transaction(account.clone())
        .authenticated_input_note(note.id())
        .build()?
        .execute()
        .await;

    assert_transaction_executor_error!(result, ERR_AUTH_PASS_THROUGH_ACCOUNT_CREATED_WITH_ASSETS);

    Ok(())
}
