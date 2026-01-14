use alloc::vec::Vec;

use miden_protocol::account::component::StorageSchema;
use miden_protocol::account::{AccountBuilder, AccountComponent, StorageSlot, StorageSlotName};
use miden_protocol::crypto::utils::bytes_to_elements_with_padding;
use miden_protocol::utils::Serializable;
use miden_protocol::utils::sync::LazyLock;
use miden_protocol::{Hasher, LexicographicWord, Word};

use crate::account::components::storage_schema_library;

static SCHEMA_COMMITMENT_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::metadata::storage_schema")
        .expect("storage slot name should be valid")
});

/// An [`AccountComponent`] exposing the account storage schema commitment.
///
/// It reexports the `get_schema_commitment` procedure from
/// `miden::standards::metadata::storage_schema`.
///
/// ## Storage Layout
///
/// - [`Self::schema_commitment_slot`]: Storage schema commitment.
pub struct AccountSchemaCommitment {
    schema_commitment: Word,
}

impl AccountSchemaCommitment {
    /// Creates a new [`AccountSchemaCommitment`] component from a list of storage schemas.
    ///
    /// The input schemas are ordered deterministically by their commitments before the final
    /// commitment is computed.
    pub fn new<'a, I>(schemas: I) -> Self
    where
        I: IntoIterator<Item = &'a StorageSchema>,
    {
        Self {
            schema_commitment: compute_schema_commitment_iter(schemas),
        }
    }

    /// Creates a new [`AccountSchemaCommitment`] component from a [`StorageSchema`].
    pub fn from_schema(storage_schema: &StorageSchema) -> Self {
        Self::new(core::slice::from_ref(storage_schema))
    }

    /// Returns the [`StorageSlotName`] where the schema commitment is stored.
    pub fn schema_commitment_slot() -> &'static StorageSlotName {
        &SCHEMA_COMMITMENT_SLOT_NAME
    }
}

impl From<AccountSchemaCommitment> for AccountComponent {
    fn from(schema_commitment: AccountSchemaCommitment) -> Self {
        AccountComponent::new(
            storage_schema_library(),
            vec![StorageSlot::with_value(
                AccountSchemaCommitment::schema_commitment_slot().clone(),
                schema_commitment.schema_commitment,
            )],
        )
        .expect(
            "AccountSchemaCommitment component should satisfy the requirements of a valid account component",
        )
        .with_supports_all_types()
    }
}

/// Extension helpers for attaching an account schema commitment component to an [`AccountBuilder`].
pub trait AccountBuilderSchemaExt {
    /// Adds the schema commitment component derived from the builder's components.
    ///
    /// Call this after adding all components to ensure the commitment reflects the final schema.
    /// Only components that carry a storage schema will contribute to the commitment.
    fn with_schema(self, include_schema: bool) -> Self;
}

impl AccountBuilderSchemaExt for AccountBuilder {
    fn with_schema(mut self, include_schema: bool) -> Self {
        if include_schema {
            let component = AccountSchemaCommitment::new(self.storage_schemas());
            self = self.with_component(component);
        }

        self
    }
}

fn compute_schema_commitment_iter<'a, I>(schemas: I) -> Word
where
    I: IntoIterator<Item = &'a StorageSchema>,
{
    let mut commitments: Vec<Word> = schemas.into_iter().map(StorageSchema::commitment).collect();
    if commitments.is_empty() {
        return Word::empty();
    }

    commitments.sort_by(|a, b| LexicographicWord::new(*a).cmp(&LexicographicWord::new(*b)));

    let mut bytes = Vec::with_capacity(commitments.len() * Word::SERIALIZED_SIZE);
    for commitment in commitments.iter() {
        commitment.write_into(&mut bytes);
    }

    let elements = bytes_to_elements_with_padding(&bytes);
    Hasher::hash_elements(&elements)
}

#[cfg(test)]
mod tests {
    use miden_protocol::Word;
    use miden_protocol::account::component::{AccountComponentMetadata, InitStorageData};
    use miden_protocol::account::{AccountBuilder, AccountComponent, AccountComponentCode};

    use super::{AccountBuilderSchemaExt, AccountSchemaCommitment};
    use crate::account::auth::NoAuth;
    use crate::account::components::storage_schema_library;

    #[test]
    fn storage_schema_commitment_is_order_independent() {
        let toml_a = r#"
            name = "Component A"
            description = "Component A schema"
            version = "0.1.0"
            supported-types = []

            [[storage.slots]]
            name = "test::slot_a"
            type = "word"
        "#;

        let toml_b = r#"
            name = "Component B"
            description = "Component B schema"
            version = "0.1.0"
            supported-types = []

            [[storage.slots]]
            name = "test::slot_b"
            description = "description is committed to"
            type = "word"
        "#;

        let metadata_a = AccountComponentMetadata::from_toml(toml_a).unwrap();
        let metadata_b = AccountComponentMetadata::from_toml(toml_b).unwrap();

        let schema_a = metadata_a.storage_schema().clone();
        let schema_b = metadata_b.storage_schema().clone();

        // Create one component for each of two different accounts, but switch orderings
        let component_a = AccountSchemaCommitment::new(&[schema_a.clone(), schema_b.clone()]);
        let component_b = AccountSchemaCommitment::new(&[schema_b, schema_a]);

        let account_a = AccountBuilder::new([1u8; 32])
            .with_auth_component(NoAuth)
            .with_component(component_a)
            .build()
            .unwrap();

        let account_b = AccountBuilder::new([2u8; 32])
            .with_auth_component(NoAuth)
            .with_component(component_b)
            .build()
            .unwrap();

        let slot_name = AccountSchemaCommitment::schema_commitment_slot();
        let commitment_a = account_a.storage().get_item(slot_name).unwrap();
        let commitment_b = account_b.storage().get_item(slot_name).unwrap();

        assert_eq!(commitment_a, commitment_b);
    }

    #[test]
    fn storage_schema_commitment_is_empty_for_no_schemas() {
        let component = AccountSchemaCommitment::new(&[]);

        assert_eq!(component.schema_commitment, Word::empty());
    }

    #[test]
    fn account_builder_with_schema_attaches_commitment_component() {
        let toml = r#"
            name = "Component C"
            description = "Component C schema"
            version = "0.1.0"
            supported-types = ["RegularAccountUpdatableCode"]

            [[storage.slots]]
            name = "test::slot_c"
            type = "word"
        "#;

        let metadata = AccountComponentMetadata::from_toml(toml).unwrap();
        let component_code = AccountComponentCode::from(storage_schema_library());
        let component =
            AccountComponent::from_library(&component_code, &metadata, &InitStorageData::default())
                .unwrap();

        let account = AccountBuilder::new([3u8; 32])
            .with_auth_component(NoAuth)
            .with_component(component)
            .with_schema(true)
            .build()
            .unwrap();

        let schema_component: AccountComponent =
            AccountSchemaCommitment::new([metadata.storage_schema()]).into();
        let expected_commitment = schema_component
            .storage_slots()
            .iter()
            .find(|slot| slot.name() == AccountSchemaCommitment::schema_commitment_slot())
            .expect("schema commitment slot should exist")
            .value();
        let stored_commitment = account
            .storage()
            .get_item(AccountSchemaCommitment::schema_commitment_slot())
            .unwrap();

        assert_eq!(stored_commitment, expected_commitment);
    }
}
