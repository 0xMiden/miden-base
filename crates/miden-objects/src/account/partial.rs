use miden_core::utils::{Deserializable, Serializable};
use miden_core::{Felt, ZERO};

use super::{Account, AccountCode, AccountId, PartialStorage};
use crate::Word;
use crate::account::hash_account;
use crate::asset::PartialVault;

/// A partial representation of an account.
///
/// A partial account is used as inputs to the transaction kernel and contains only the essential
/// data needed for verification and transaction processing without requiring the full account
/// state.
///
/// For new accounts, the partial storage must be the full initial account storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialAccount {
    /// The ID for the partial account
    id: AccountId,
    /// The current transaction nonce of the account
    nonce: Felt,
    /// Account code
    account_code: AccountCode,
    /// Partial representation of the account's storage, containing the storage commitment and
    /// proofs for specific storage slots that need to be accessed
    partial_storage: PartialStorage,
    /// Partial representation of the account's vault, containing the vault root and necessary
    /// proof information for asset verification
    partial_vault: PartialVault,
}

impl PartialAccount {
    /// Creates a new instance of a partial account with the specified components.
    pub fn new(
        id: AccountId,
        nonce: Felt,
        account_code: AccountCode,
        partial_storage: PartialStorage,
        partial_vault: PartialVault,
    ) -> Self {
        Self {
            id,
            nonce,
            account_code,
            partial_storage,
            partial_vault,
        }
    }

    /// Returns the account's unique identifier.
    pub fn id(&self) -> AccountId {
        self.id
    }

    /// Returns the account's current nonce value.
    pub fn nonce(&self) -> Felt {
        self.nonce
    }

    /// Returns a reference to the account code.
    pub fn code(&self) -> &AccountCode {
        &self.account_code
    }

    /// Returns a reference to the partial storage representation of the account.
    pub fn storage(&self) -> &PartialStorage {
        &self.partial_storage
    }

    /// Returns a reference to the partial vault representation of the account.
    pub fn vault(&self) -> &PartialVault {
        &self.partial_vault
    }

    /// Returns true if the account is new (i.e. its nonce is zero and it hasn't been registered on
    /// chain yet).
    pub fn is_new(&self) -> bool {
        self.nonce == ZERO
    }

    /// Returns the commitment of this account.
    ///
    /// The commitment of an account is computed as:
    ///
    /// ```text
    /// hash(id, nonce, vault_root, storage_commitment, code_commitment).
    /// ```
    pub fn commitment(&self) -> Word {
        hash_account(
            self.id,
            self.nonce,
            self.vault().root(),
            self.storage().commitment(),
            self.code().commitment(),
        )
    }

    /// Returns the commitment of this account as used for the initial account state commitment in
    /// transaction proofs.
    ///
    /// For existing accounts, this is exactly the same as [Account::commitment()], however, for new
    /// accounts this value is set to [`Word::empty`]. This is because when a transaction is
    /// executed against a new account, public input for the initial account state is set to
    /// [`Word::empty`] to distinguish new accounts from existing accounts. The actual
    /// commitment of the initial account state (and the initial state itself), are provided to
    /// the VM via the advice provider.
    pub fn initial_commitment(&self) -> Word {
        if self.is_new() {
            Word::empty()
        } else {
            self.commitment()
        }
    }

    /// Returns `true` if the full state of the account is public on chain, and `false` otherwise.
    pub fn has_public_state(&self) -> bool {
        self.id.has_public_state()
    }

    /// Consumes self and returns the underlying parts of the partial account.
    pub fn into_parts(self) -> (AccountId, Felt, AccountCode, PartialStorage, PartialVault) {
        (self.id, self.nonce, self.account_code, self.partial_storage, self.partial_vault)
    }
}

impl From<Account> for PartialAccount {
    fn from(account: Account) -> Self {
        PartialAccount::from(&account)
    }
}

impl From<&Account> for PartialAccount {
    fn from(account: &Account) -> Self {
        PartialAccount::new(
            account.id(),
            account.nonce(),
            account.code().clone(),
            account.storage().into(),
            account.vault().into(),
        )
    }
}

impl Serializable for PartialAccount {
    fn write_into<W: miden_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(self.id);
        target.write(self.nonce);
        target.write(&self.account_code);
        target.write(&self.partial_storage);
        target.write(&self.partial_vault);
    }
}

impl Deserializable for PartialAccount {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, miden_processor::DeserializationError> {
        let account_id = source.read()?;
        let nonce = source.read()?;
        let account_code = source.read()?;
        let partial_storage = source.read()?;
        let partial_vault = source.read()?;

        Ok(PartialAccount {
            id: account_id,
            nonce,
            account_code,
            partial_storage,
            partial_vault,
        })
    }
}
