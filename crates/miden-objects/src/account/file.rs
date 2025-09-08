use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

use miden_crypto::utils::SliceReader;

use super::super::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use super::{Account, AccountId, AuthSecretKey, Word};

const MAGIC: &str = "acct";
const ACCOUNT_EXPORT_MAGIC: &str = "aexp";
const AUTH_EXPORT_MAGIC: &str = "auth";

// ACCOUNT FILE
// ================================================================================================

/// Account file contains a complete description of an account, including the [`Account`] struct as
/// well as account seed and account authentication info.
///
/// The account authentication info consists of a list of [`AuthSecretKey`] that the account may
/// use within its code.
///
/// The intent of this struct is to provide an easy way to serialize and deserialize all
/// account-related data as a single unit (e.g., to/from files).
#[derive(Debug, Clone)]
pub struct AccountFile {
    pub account: Account,
    pub account_seed: Option<Word>,
    pub auth_secret_keys: Vec<AuthSecretKey>,
}

impl AccountFile {
    pub fn new(
        account: Account,
        account_seed: Option<Word>,
        auth_keys: Vec<AuthSecretKey>,
    ) -> Self {
        Self {
            account,
            account_seed,
            auth_secret_keys: auth_keys,
        }
    }
}

// ACCOUNT EXPORT
// ================================================================================================

/// Account export contains account data without private keys.
///
/// This struct provides a secure way to export account information without exposing
/// sensitive authentication data. It includes the account state and optional seed
/// but excludes private keys.
#[derive(Debug, Clone)]
pub struct AccountExport {
    pub account: Account,
    pub account_seed: Option<Word>,
}

impl AccountExport {
    pub fn new(account: Account, account_seed: Option<Word>) -> Self {
        Self { account, account_seed }
    }

    /// Create an AccountExport from an AccountFile, excluding private keys
    pub fn from_account_file(account_file: &AccountFile) -> Self {
        Self::new(account_file.account.clone(), account_file.account_seed)
    }
}

// AUTHENTICATION EXPORT
// ================================================================================================

/// Authentication export contains private keys associated with an account.
///
/// This struct provides a way to export authentication data separately from account data.
/// It includes the account ID for association and the private keys used for authentication.
#[derive(Debug, Clone)]
pub struct AuthenticationExport {
    pub account_id: AccountId,
    pub auth_secret_keys: Vec<AuthSecretKey>,
}

impl AuthenticationExport {
    pub fn new(account_id: AccountId, auth_secret_keys: Vec<AuthSecretKey>) -> Self {
        Self { account_id, auth_secret_keys }
    }

    /// Create an AuthenticationExport from an AccountFile
    pub fn from_account_file(account_file: &AccountFile) -> Self {
        Self::new(account_file.account.id(), account_file.auth_secret_keys.clone())
    }
}

#[cfg(feature = "std")]
impl AccountFile {
    /// Serializes and writes binary [AccountFile] to specified file
    pub fn write(&self, filepath: impl AsRef<Path>) -> io::Result<()> {
        fs::write(filepath, self.to_bytes())
    }

    /// Reads from file and tries to deserialize an [AccountFile]
    pub fn read(filepath: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(filepath)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;
        let mut reader = SliceReader::new(&buffer);

        Ok(AccountFile::read_from(&mut reader).map_err(|_| io::ErrorKind::InvalidData)?)
    }
}

#[cfg(feature = "std")]
impl AccountExport {
    /// Serializes and writes binary AccountExport to specified file
    pub fn write(&self, filepath: impl AsRef<Path>) -> io::Result<()> {
        fs::write(filepath, self.to_bytes())
    }

    /// Reads from file and tries to deserialize an AccountExport
    pub fn read(filepath: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(filepath)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;
        let mut reader = SliceReader::new(&buffer);

        Ok(AccountExport::read_from(&mut reader).map_err(|_| io::ErrorKind::InvalidData)?)
    }
}

#[cfg(feature = "std")]
impl AuthenticationExport {
    /// Serializes and writes binary AuthenticationExport to specified file
    pub fn write(&self, filepath: impl AsRef<Path>) -> io::Result<()> {
        fs::write(filepath, self.to_bytes())
    }

    /// Reads from file and tries to deserialize an AuthenticationExport
    pub fn read(filepath: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(filepath)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;
        let mut reader = SliceReader::new(&buffer);

        Ok(AuthenticationExport::read_from(&mut reader).map_err(|_| io::ErrorKind::InvalidData)?)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountFile {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_bytes(MAGIC.as_bytes());
        let AccountFile {
            account,
            account_seed,
            auth_secret_keys: auth,
        } = self;

        account.write_into(target);
        account_seed.write_into(target);
        auth.write_into(target);
    }
}

impl Deserializable for AccountFile {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let magic_value = source.read_string(4)?;
        if magic_value != MAGIC {
            return Err(DeserializationError::InvalidValue(format!(
                "invalid account file marker: {magic_value}"
            )));
        }
        let account = Account::read_from(source)?;
        let account_seed = <Option<Word>>::read_from(source)?;
        let auth_secret_keys = <Vec<AuthSecretKey>>::read_from(source)?;

        Ok(Self::new(account, account_seed, auth_secret_keys))
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, DeserializationError> {
        Self::read_from(&mut SliceReader::new(bytes))
    }
}

// SERIALIZATION FOR ACCOUNT EXPORT
// ================================================================================================

impl Serializable for AccountExport {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_bytes(ACCOUNT_EXPORT_MAGIC.as_bytes());
        let AccountExport { account, account_seed } = self;

        account.write_into(target);
        account_seed.write_into(target);
    }
}

impl Deserializable for AccountExport {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let magic_value = source.read_string(4)?;
        if magic_value != ACCOUNT_EXPORT_MAGIC {
            return Err(DeserializationError::InvalidValue(format!(
                "invalid account export file marker: {magic_value}"
            )));
        }
        let account = Account::read_from(source)?;
        let account_seed = <Option<Word>>::read_from(source)?;

        Ok(Self::new(account, account_seed))
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, DeserializationError> {
        Self::read_from(&mut SliceReader::new(bytes))
    }
}

// SERIALIZATION FOR AUTHENTICATION EXPORT
// ================================================================================================

impl Serializable for AuthenticationExport {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_bytes(AUTH_EXPORT_MAGIC.as_bytes());
        let AuthenticationExport { account_id, auth_secret_keys } = self;

        account_id.write_into(target);
        auth_secret_keys.write_into(target);
    }
}

impl Deserializable for AuthenticationExport {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let magic_value = source.read_string(4)?;
        if magic_value != AUTH_EXPORT_MAGIC {
            return Err(DeserializationError::InvalidValue(format!(
                "invalid authentication export file marker: {magic_value}"
            )));
        }
        let account_id = AccountId::read_from(source)?;
        let auth_secret_keys = <Vec<AuthSecretKey>>::read_from(source)?;

        Ok(Self::new(account_id, auth_secret_keys))
    }

    fn read_from_bytes(bytes: &[u8]) -> Result<Self, DeserializationError> {
        Self::read_from(&mut SliceReader::new(bytes))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_crypto::dsa::rpo_falcon512::SecretKey;
    use miden_crypto::utils::{Deserializable, Serializable};
    use storage::AccountStorage;
    #[cfg(feature = "std")]
    use tempfile::tempdir;

    use super::{AccountExport, AccountFile, AuthenticationExport};
    use crate::account::{Account, AccountCode, AccountId, AuthSecretKey, Felt, Word, storage};
    use crate::asset::AssetVault;
    use crate::testing::account_id::ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE;

    fn build_account_file() -> AccountFile {
        let id = AccountId::try_from(ACCOUNT_ID_REGULAR_PUBLIC_ACCOUNT_IMMUTABLE_CODE).unwrap();
        let code = AccountCode::mock();

        // create account and auth
        let vault = AssetVault::new(&[]).unwrap();
        let storage = AccountStorage::new(vec![]).unwrap();
        let nonce = Felt::new(0);
        let account = Account::from_parts(id, vault, storage, code, nonce);
        let account_seed = Some(Word::empty());
        let auth_secret_key = AuthSecretKey::RpoFalcon512(SecretKey::new());
        let auth_secret_key_2 = AuthSecretKey::RpoFalcon512(SecretKey::new());

        AccountFile::new(account, account_seed, vec![auth_secret_key, auth_secret_key_2])
    }

    #[test]
    fn test_serde() {
        let account_file = build_account_file();
        let serialized = account_file.to_bytes();
        let deserialized = AccountFile::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized.account, account_file.account);
        assert_eq!(deserialized.account_seed, account_file.account_seed);
        assert_eq!(
            deserialized.auth_secret_keys.to_bytes(),
            account_file.auth_secret_keys.to_bytes()
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_serde_file() {
        let dir = tempdir().unwrap();
        let filepath = dir.path().join("account_file.mac");

        let account_file = build_account_file();
        account_file.write(filepath.as_path()).unwrap();
        let deserialized = AccountFile::read(filepath.as_path()).unwrap();

        assert_eq!(deserialized.account, account_file.account);
        assert_eq!(deserialized.account_seed, account_file.account_seed);
        assert_eq!(
            deserialized.auth_secret_keys.to_bytes(),
            account_file.auth_secret_keys.to_bytes()
        );
    }

    #[test]
    fn test_account_export_serde() {
        let account_file = build_account_file();
        let account_export = AccountExport::from_account_file(&account_file);

        let serialized = account_export.to_bytes();
        let deserialized = AccountExport::read_from_bytes(&serialized).unwrap();

        assert_eq!(deserialized.account, account_export.account);
        assert_eq!(deserialized.account_seed, account_export.account_seed);
    }

    #[test]
    fn test_authentication_export_serde() {
        let account_file = build_account_file();
        let auth_export = AuthenticationExport::from_account_file(&account_file);

        let serialized = auth_export.to_bytes();
        let deserialized = AuthenticationExport::read_from_bytes(&serialized).unwrap();

        assert_eq!(deserialized.account_id, auth_export.account_id);
        assert_eq!(
            deserialized.auth_secret_keys.to_bytes(),
            auth_export.auth_secret_keys.to_bytes()
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_account_export_file() {
        let dir = tempdir().unwrap();
        let filepath = dir.path().join("account_export.adf");

        let account_file = build_account_file();
        let account_export = AccountExport::from_account_file(&account_file);

        account_export.write(filepath.as_path()).unwrap();
        let deserialized = AccountExport::read(filepath.as_path()).unwrap();

        assert_eq!(deserialized.account, account_export.account);
        assert_eq!(deserialized.account_seed, account_export.account_seed);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_authentication_export_file() {
        let dir = tempdir().unwrap();
        let filepath = dir.path().join("auth_export.akf");

        let account_file = build_account_file();
        let auth_export = AuthenticationExport::from_account_file(&account_file);

        auth_export.write(filepath.as_path()).unwrap();
        let deserialized = AuthenticationExport::read(filepath.as_path()).unwrap();

        assert_eq!(deserialized.account_id, auth_export.account_id);
        assert_eq!(
            deserialized.auth_secret_keys.to_bytes(),
            auth_export.auth_secret_keys.to_bytes()
        );
    }
}
