use miden_lib::{AuthScheme, account::wallets::create_basic_wallet};
use miden_objects::{Word, crypto::dsa::rpo_falcon512::SecretKey};
use rand_chacha::{ChaCha20Rng, rand_core::SeedableRng};

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn wallet_creation() {
    use miden_lib::account::{auth::RpoFalcon512, wallets::BasicWallet};
    use miden_objects::account::{AccountCode, AccountStorageMode, AccountType};

    // we need a Falcon Public Key to create the wallet account
    let seed = [0_u8; 32];
    let mut rng = ChaCha20Rng::from_seed(seed);

    let sec_key = SecretKey::with_rng(&mut rng);
    let pub_key = sec_key.public_key();
    let auth_scheme: AuthScheme = AuthScheme::RpoFalcon512 { pub_key };

    // we need to use an initial seed to create the wallet account
    let init_seed: [u8; 32] = [
        95, 113, 209, 94, 84, 105, 250, 242, 223, 203, 216, 124, 22, 159, 14, 132, 215, 85, 183,
        204, 149, 90, 166, 68, 100, 73, 106, 168, 125, 237, 138, 16,
    ];

    let account_type = AccountType::RegularAccountImmutableCode;
    let storage_mode = AccountStorageMode::Private;

    let (wallet, _) =
        create_basic_wallet(init_seed, auth_scheme, account_type, storage_mode).unwrap();

    let expected_code = AccountCode::from_components(
        &[RpoFalcon512::new(pub_key).into(), BasicWallet.into()],
        AccountType::RegularAccountUpdatableCode,
    )
    .unwrap();
    let expected_code_commitment = expected_code.commitment();

    assert!(wallet.is_regular_account());
    assert_eq!(wallet.code().commitment(), expected_code_commitment);
    let pub_key_word: Word = pub_key.into();
    assert_eq!(wallet.storage().get_item(0).unwrap().as_elements(), pub_key_word);
}
