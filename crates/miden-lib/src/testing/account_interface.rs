use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::Account;

use crate::AuthScheme;
use crate::account::interface::AccountInterface;

/// Helper function to extract public keys from an account
pub fn get_public_keys_from_account(account: &Account) -> Vec<Word> {
    let mut pub_keys = vec![];
    let interface: AccountInterface = account.into();

    for auth in interface.auth() {
        match auth {
            AuthScheme::NoAuth => {},
            AuthScheme::RpoFalcon512 { pub_key } => pub_keys.push(Word::from(*pub_key)),
            AuthScheme::RpoFalcon512Multisig { pub_keys: multisig_keys, .. } => {
                for key in multisig_keys {
                    pub_keys.push(Word::from(*key));
                }
            },
            AuthScheme::Unknown => {},
        }
    }

    pub_keys
}
