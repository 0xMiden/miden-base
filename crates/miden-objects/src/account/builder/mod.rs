use alloc::{boxed::Box, vec::Vec};

use vm_core::FieldElement;
use vm_processor::Digest;

use crate::{
    AccountError, Felt, Word,
    account::{
        Account, AccountCode, AccountComponent, AccountId, AccountIdV0, AccountIdVersion,
        AccountStorage, AccountStorageMode, AccountType,
    },
    asset::AssetVault,
};

/// A convenient builder for an [`Account`] allowing for safe construction of an account by
/// combining multiple [`AccountComponent`]s.
///
/// This will build a valid new account with these properties:
/// - An empty [`AssetVault`].
/// - The nonce set to [`Felt::ZERO`].
/// - A seed which results in an [`AccountId`] valid for the configured account type and storage
///   mode.
///
/// By default, the builder is initialized with:
/// - The `account_type` set to [`AccountType::RegularAccountUpdatableCode`].
/// - The `storage_mode` set to [`AccountStorageMode::Private`].
/// - The `version` set to [`AccountIdVersion::Version0`].
///
/// The methods that are required to be called are:
///
/// - [`AccountBuilder::with_auth_component`],
/// - [`AccountBuilder::with_component`], which must be called at least once.
///
/// Under the `testing` feature, it is possible to:
/// - Build an existing account using [`AccountBuilder::build_existing`] which will set the
///   account's nonce to `1`.
/// - Add assets to the account's vault, however this will only succeed when using
///   [`AccountBuilder::build_existing`].
#[derive(Debug, Clone)]
pub struct AccountBuilder {
    #[cfg(any(feature = "testing", test))]
    assets: Vec<crate::asset::Asset>,
    components: Vec<AccountComponent>,
    auth_component: Option<AccountComponent>,
    account_type: AccountType,
    storage_mode: AccountStorageMode,
    init_seed: [u8; 32],
    id_version: AccountIdVersion,
}

impl AccountBuilder {
    /// Creates a new builder for an account and sets the initial seed from which the grinding
    /// process for that account's [`AccountId`] will start.
    ///
    /// This initial seed should come from a cryptographic random number generator.
    pub fn new(init_seed: [u8; 32]) -> Self {
        Self {
            #[cfg(any(feature = "testing", test))]
            assets: vec![],
            components: vec![],
            auth_component: None,
            init_seed,
            account_type: AccountType::RegularAccountUpdatableCode,
            storage_mode: AccountStorageMode::Private,
            id_version: AccountIdVersion::Version0,
        }
    }

    /// Sets the [`AccountIdVersion`] of the account ID.
    pub fn version(mut self, version: AccountIdVersion) -> Self {
        self.id_version = version;
        self
    }

    /// Sets the type of the account.
    pub fn account_type(mut self, account_type: AccountType) -> Self {
        self.account_type = account_type;
        self
    }

    /// Sets the storage mode of the account.
    pub fn storage_mode(mut self, storage_mode: AccountStorageMode) -> Self {
        self.storage_mode = storage_mode;
        self
    }

    /// Adds an [`AccountComponent`] to the builder. This method can be called multiple times and
    /// **must be called at least once** since an account must export at least one procedure.
    ///
    /// All components will be merged to form the final code and storage of the built account.
    pub fn with_component(mut self, account_component: impl Into<AccountComponent>) -> Self {
        self.components.push(account_component.into());
        self
    }

    /// Adds a designated authentication [`AccountComponent`] to the builder.
    ///
    /// This component may contain multiple procedures, but is expected to contain exactly one
    /// authentication procedure (named `auth__*`).
    /// Calling this method multiple times will override the previous auth component.
    ///
    /// Procedures from this component will be placed at the beginning of the account procedure
    /// list.
    pub fn with_auth_component(mut self, account_component: impl Into<AccountComponent>) -> Self {
        self.auth_component = Some(account_component.into());
        self
    }

    /// Builds the common parts of testing and non-testing code.
    fn build_inner(&mut self) -> Result<(AssetVault, AccountCode, AccountStorage), AccountError> {
        #[cfg(any(feature = "testing", test))]
        let vault = AssetVault::new(&self.assets).map_err(|err| {
            AccountError::BuildError(format!("asset vault failed to build: {err}"), None)
        })?;

        #[cfg(all(not(feature = "testing"), not(test)))]
        let vault = AssetVault::default();

        let auth_component = self
            .auth_component
            .take()
            .ok_or(AccountError::BuildError("auth component must be set".into(), None))?;

        let mut components = vec![auth_component];
        components.append(&mut self.components);

        let (code, storage) = Account::initialize_from_components(self.account_type, &components)
            .map_err(|err| {
            AccountError::BuildError(
                "account components failed to build".into(),
                Some(Box::new(err)),
            )
        })?;

        Ok((vault, code, storage))
    }

    /// Grinds a new [`AccountId`] using the `init_seed` as a starting point.
    fn grind_account_id(
        &self,
        init_seed: [u8; 32],
        version: AccountIdVersion,
        code_commitment: Digest,
        storage_commitment: Digest,
    ) -> Result<Word, AccountError> {
        let seed = AccountIdV0::compute_account_seed(
            init_seed,
            self.account_type,
            self.storage_mode,
            version,
            code_commitment,
            storage_commitment,
        )
        .map_err(|err| {
            AccountError::BuildError("account seed generation failed".into(), Some(Box::new(err)))
        })?;

        Ok(seed)
    }

    /// Builds an [`Account`] out of the configured builder.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The init seed is not set.
    /// - Any of the components does not support the set account type.
    /// - The number of procedures in all merged components is 0 or exceeds
    ///   [`AccountCode::MAX_NUM_PROCEDURES`](crate::account::AccountCode::MAX_NUM_PROCEDURES).
    /// - Two or more libraries export a procedure with the same MAST root.
    /// - Authentication component is missing.
    /// - Multiple authentication procedures are found.
    /// - The number of [`StorageSlot`](crate::account::StorageSlot)s of all components exceeds 255.
    /// - [`MastForest::merge`](vm_processor::MastForest::merge) fails on the given components.
    /// - If duplicate assets were added to the builder (only under the `testing` feature).
    /// - If the vault is not empty on new accounts (only under the `testing` feature).
    pub fn build(mut self) -> Result<(Account, Word), AccountError> {
        let (vault, code, storage) = self.build_inner()?;

        #[cfg(any(feature = "testing", test))]
        if !vault.is_empty() {
            return Err(AccountError::BuildError(
                "account asset vault must be empty on new accounts".into(),
                None,
            ));
        }

        let seed = self.grind_account_id(
            self.init_seed,
            self.id_version,
            code.commitment(),
            storage.commitment(),
        )?;

        let account_id = AccountId::new(
            seed,
            AccountIdVersion::Version0,
            code.commitment(),
            storage.commitment(),
        )
        .expect("get_account_seed should provide a suitable seed");

        debug_assert_eq!(account_id.account_type(), self.account_type);
        debug_assert_eq!(account_id.storage_mode(), self.storage_mode);

        let account = Account::from_parts(account_id, vault, storage, code, Felt::ZERO);

        Ok((account, seed))
    }
}

#[cfg(any(feature = "testing", test))]
impl AccountBuilder {
    /// Adds all the assets to the account's [`AssetVault`]. This method is optional.
    ///
    /// Must only be used when using [`Self::build_existing`] instead of [`Self::build`] since new
    /// accounts must have an empty vault.
    pub fn with_assets<I: IntoIterator<Item = crate::asset::Asset>>(mut self, assets: I) -> Self {
        self.assets.extend(assets);
        self
    }

    /// Builds the account as an existing account, that is, with the nonce set to [`Felt::ONE`].
    ///
    /// The [`AccountId`] is constructed by slightly modifying `init_seed[0..8]` to be a valid ID.
    ///
    /// For possible errors, see the documentation of [`Self::build`].
    pub fn build_existing(mut self) -> Result<Account, AccountError> {
        let (vault, code, storage) = self.build_inner()?;

        let account_id = {
            let bytes = <[u8; 15]>::try_from(&self.init_seed[0..15])
                .expect("we should have sliced exactly 15 bytes off");
            AccountId::dummy(
                bytes,
                AccountIdVersion::Version0,
                self.account_type,
                self.storage_mode,
            )
        };

        Ok(Account::from_parts(account_id, vault, storage, code, Felt::ONE))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use assembly::{Assembler, Library};
    use assert_matches::assert_matches;
    use vm_core::FieldElement;

    use super::*;
    use crate::{account::StorageSlot, testing::account_component::NoopAuthComponent};

    const CUSTOM_CODE1: &str = "
          export.foo
            push.2.2 add eq.4
          end
        ";
    const CUSTOM_CODE2: &str = "
            export.bar
              push.4.4 add eq.8
            end
          ";

    static CUSTOM_LIBRARY1: LazyLock<Library> = LazyLock::new(|| {
        Assembler::default()
            .assemble_library([CUSTOM_CODE1])
            .expect("code should be valid")
    });
    static CUSTOM_LIBRARY2: LazyLock<Library> = LazyLock::new(|| {
        Assembler::default()
            .assemble_library([CUSTOM_CODE2])
            .expect("code should be valid")
    });

    struct CustomComponent1 {
        slot0: u64,
    }
    impl From<CustomComponent1> for AccountComponent {
        fn from(custom: CustomComponent1) -> Self {
            let mut value = Word::default();
            value[0] = Felt::new(custom.slot0);

            AccountComponent::new(CUSTOM_LIBRARY1.clone(), vec![StorageSlot::Value(value)])
                .expect("component should be valid")
                .with_supports_all_types()
        }
    }

    struct CustomComponent2 {
        slot0: u64,
        slot1: u64,
    }
    impl From<CustomComponent2> for AccountComponent {
        fn from(custom: CustomComponent2) -> Self {
            let mut value0 = Word::default();
            value0[3] = Felt::new(custom.slot0);
            let mut value1 = Word::default();
            value1[3] = Felt::new(custom.slot1);

            AccountComponent::new(
                CUSTOM_LIBRARY2.clone(),
                vec![StorageSlot::Value(value0), StorageSlot::Value(value1)],
            )
            .expect("component should be valid")
            .with_supports_all_types()
        }
    }

    #[test]
    fn account_builder() {
        let storage_slot0 = 25;
        let storage_slot1 = 12;
        let storage_slot2 = 42;

        let (account, seed) = Account::builder([5; 32])
            .with_auth_component(NoopAuthComponent::new(Assembler::default()).unwrap())
            .with_component(CustomComponent1 { slot0: storage_slot0 })
            .with_component(CustomComponent2 {
                slot0: storage_slot1,
                slot1: storage_slot2,
            })
            .build()
            .unwrap();

        // Account should be new, i.e. nonce = zero.
        assert_eq!(account.nonce(), Felt::ZERO);

        let computed_id = AccountId::new(
            seed,
            AccountIdVersion::Version0,
            account.code.commitment(),
            account.storage.commitment(),
        )
        .unwrap();
        assert_eq!(account.id(), computed_id);

        // The merged code should have one procedure from each library.
        assert_eq!(account.code.procedure_roots().count(), 3);

        let foo_root = CUSTOM_LIBRARY1.mast_forest()
            [CUSTOM_LIBRARY1.get_export_node_id(CUSTOM_LIBRARY1.exports().next().unwrap())]
        .digest();
        let bar_root = CUSTOM_LIBRARY2.mast_forest()
            [CUSTOM_LIBRARY2.get_export_node_id(CUSTOM_LIBRARY2.exports().next().unwrap())]
        .digest();

        let foo_procedure_info = &account
            .code()
            .procedures()
            .iter()
            .find(|info| info.mast_root() == &foo_root)
            .unwrap();
        assert_eq!(foo_procedure_info.storage_offset(), 0);
        assert_eq!(foo_procedure_info.storage_size(), 1);

        let bar_procedure_info = &account
            .code()
            .procedures()
            .iter()
            .find(|info| info.mast_root() == &bar_root)
            .unwrap();
        assert_eq!(bar_procedure_info.storage_offset(), 1);
        assert_eq!(bar_procedure_info.storage_size(), 2);

        assert_eq!(
            account.storage().get_item(0).unwrap(),
            [Felt::new(storage_slot0), Felt::new(0), Felt::new(0), Felt::new(0)].into()
        );
        assert_eq!(
            account.storage().get_item(1).unwrap(),
            [Felt::new(0), Felt::new(0), Felt::new(0), Felt::new(storage_slot1)].into()
        );
        assert_eq!(
            account.storage().get_item(2).unwrap(),
            [Felt::new(0), Felt::new(0), Felt::new(0), Felt::new(storage_slot2)].into()
        );
    }

    #[test]
    fn account_builder_non_empty_vault_on_new_account() {
        let storage_slot0 = 25;

        let build_error = Account::builder([0xff; 32])
            .with_auth_component(NoopAuthComponent::new(Assembler::default()).unwrap())
            .with_component(CustomComponent1 { slot0: storage_slot0 })
            .with_assets(AssetVault::mock().assets())
            .build()
            .unwrap_err();

        assert_matches!(build_error, AccountError::BuildError(msg, _) if msg == "account asset vault must be empty on new accounts")
    }

    // TODO: Test that a BlockHeader with a number which is not a multiple of 2^16 returns an error.
}
