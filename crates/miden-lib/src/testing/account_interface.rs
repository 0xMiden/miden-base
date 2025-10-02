use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::Account;

use crate::account::interface::AccountInterface;

/// Helper function to extract public keys from an account
pub fn get_public_keys_from_account(account: &Account) -> Vec<Word> {
    let interface: AccountInterface = account.into();

    interface
        .auth()
        .iter()
        .flat_map(|auth| auth.get_public_key_commitments())
        .collect()
}
