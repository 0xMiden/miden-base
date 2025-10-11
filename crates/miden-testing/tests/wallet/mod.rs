use miden_lib::AuthScheme;
use miden_lib::account::wallets::create_basic_wallet;
use miden_objects::Word;
use miden_objects::account::auth::AuthSecretKey;
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn wallet_creation() {
    use miden_lib::account::auth::AuthRpoFalcon512;
    use miden_lib::account::wallets::BasicWallet;
    use miden_objects::account::{AccountCode, AccountStorageMode, AccountType};

    // we need a Falcon Public Key to create the wallet account
    let seed = [0_u8; 32];
    let mut rng = ChaCha20Rng::from_seed(seed);

    let sec_key = AuthSecretKey::rpo_falcon512_with_rng(&mut rng);
    let pub_key = sec_key.public_key().to_commitment();
    let auth_scheme: AuthScheme = AuthScheme::RpoFalcon512 { pub_key };

    // we need to use an initial seed to create the wallet account
    let init_seed: [u8; 32] = [
        95, 113, 209, 94, 84, 105, 250, 242, 223, 203, 216, 124, 22, 159, 14, 132, 215, 85, 183,
        204, 149, 90, 166, 68, 100, 73, 106, 168, 125, 237, 138, 16,
    ];

    let account_type = AccountType::RegularAccountImmutableCode;
    let storage_mode = AccountStorageMode::Private;

    let wallet = create_basic_wallet(init_seed, auth_scheme, account_type, storage_mode).unwrap();

    let expected_code = AccountCode::from_components(
        &[AuthRpoFalcon512::new(pub_key).into(), BasicWallet.into()],
        AccountType::RegularAccountUpdatableCode,
    )
    .unwrap();
    let expected_code_commitment = expected_code.commitment();

    assert!(wallet.is_regular_account());
    assert_eq!(wallet.code().commitment(), expected_code_commitment);
    assert_eq!(wallet.storage().get_item(0).unwrap(), Word::from(pub_key));
}
