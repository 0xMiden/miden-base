use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_protocol::Word;
use miden_protocol::account::{
    AccountCode,
    AccountPatch,
    AccountProcedureRoot,
    AccountStoragePatch,
    AccountUpdateDetails,
    AccountVaultPatch,
    StorageMapKey,
    StorageMapPatch,
    StorageMapPatchEntries,
    StorageSlotName,
    StorageSlotPatch,
    StorageValuePatch,
};
use miden_protocol::asset::AssetId;

use super::{MessageDecodeExt, required};
use crate::{ConversionError, ConversionResultExt, proto};

// ACCOUNT CODE
// ================================================================================================

impl From<&AccountCode> for proto::account::AccountCode {
    fn from(code: &AccountCode) -> Self {
        Self {
            mast: Some(code.mast().as_ref().into()),
            procedure_roots: code.procedure_roots().map(Into::into).collect(),
        }
    }
}

impl From<AccountCode> for proto::account::AccountCode {
    fn from(code: AccountCode) -> Self {
        Self::from(&code)
    }
}

impl TryFrom<proto::account::AccountCode> for AccountCode {
    type Error = ConversionError;

    fn try_from(code: proto::account::AccountCode) -> Result<Self, Self::Error> {
        let decoder = code.decoder();
        let mast = required!(decoder, code.mast)?;
        let procedure_roots = code
            .procedure_roots
            .into_iter()
            .enumerate()
            .map(|(index, root)| {
                Word::try_from(root)
                    .map(AccountProcedureRoot::from_raw)
                    .context(format!("procedure_roots[{index}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        AccountCode::from_parts(Arc::new(mast), procedure_roots).map_err(ConversionError::new)
    }
}

// STORAGE PATCHES
// ================================================================================================

impl From<&StorageValuePatch> for proto::account::StorageValuePatch {
    fn from(patch: &StorageValuePatch) -> Self {
        use proto::account::storage_value_patch::Operation;

        let operation = match patch {
            StorageValuePatch::Create { value } => Operation::Create(value.into()),
            StorageValuePatch::Update { value } => Operation::Update(value.into()),
            StorageValuePatch::Remove => Operation::Remove(()),
        };
        Self { operation: Some(operation) }
    }
}

impl TryFrom<proto::account::StorageValuePatch> for StorageValuePatch {
    type Error = ConversionError;

    fn try_from(patch: proto::account::StorageValuePatch) -> Result<Self, Self::Error> {
        use proto::account::storage_value_patch::Operation;

        match patch.operation {
            Some(Operation::Create(value)) => Ok(Self::Create {
                value: value.try_into().context("operation.create")?,
            }),
            Some(Operation::Update(value)) => Ok(Self::Update {
                value: value.try_into().context("operation.update")?,
            }),
            Some(Operation::Remove(())) => Ok(Self::Remove),
            None => Err(ConversionError::missing_field::<proto::account::StorageValuePatch>(
                "operation",
            )),
        }
    }
}

impl From<&StorageMapPatch> for proto::account::StorageMapPatch {
    fn from(patch: &StorageMapPatch) -> Self {
        use proto::account::storage_map_patch::Operation;

        let operation = match patch {
            StorageMapPatch::Create { entries } => Operation::Create(entries.into()),
            StorageMapPatch::Update { entries } => Operation::Update(entries.into()),
            StorageMapPatch::Remove => Operation::Remove(()),
        };
        Self { operation: Some(operation) }
    }
}

impl TryFrom<proto::account::StorageMapPatch> for StorageMapPatch {
    type Error = ConversionError;

    fn try_from(patch: proto::account::StorageMapPatch) -> Result<Self, Self::Error> {
        use proto::account::storage_map_patch::Operation;

        match patch.operation {
            Some(Operation::Create(entries)) => Ok(Self::Create {
                entries: entries.try_into().context("operation.create")?,
            }),
            Some(Operation::Update(entries)) => {
                let entries: StorageMapPatchEntries =
                    entries.try_into().context("operation.update")?;
                if entries.is_empty() {
                    return Err(ConversionError::message(
                        "entries must be non-empty for an update operation",
                    )
                    .context("operation.update.entries"));
                }
                Ok(Self::Update { entries })
            },
            Some(Operation::Remove(())) => Ok(Self::Remove),
            None => {
                Err(ConversionError::missing_field::<proto::account::StorageMapPatch>("operation"))
            },
        }
    }
}

impl From<&StorageMapPatchEntries> for proto::account::StorageMapPatchEntries {
    fn from(entries: &StorageMapPatchEntries) -> Self {
        Self {
            entries: entries
                .as_map()
                .iter()
                .map(|(key, value)| proto::account::StorageMapEntry {
                    key: Some(Word::from(*key).into()),
                    value: Some(value.into()),
                })
                .collect(),
        }
    }
}

impl TryFrom<proto::account::StorageMapPatchEntries> for StorageMapPatchEntries {
    type Error = ConversionError;

    fn try_from(patch: proto::account::StorageMapPatchEntries) -> Result<Self, Self::Error> {
        let mut entries = BTreeMap::new();
        for (index, entry) in patch.entries.into_iter().enumerate() {
            let decoder = entry.decoder();
            let entry_context = format!("entries[{index}]");
            let key = StorageMapKey::from_raw(
                required!(decoder, entry.key).context(entry_context.clone())?,
            );
            let value = required!(decoder, entry.value).context(entry_context.clone())?;
            if entries.insert(key, value).is_some() {
                return Err(ConversionError::message("duplicate storage map key")
                    .context(format!("{entry_context}.key")));
            }
        }

        Ok(Self::from_raw(entries))
    }
}

impl From<&AccountStoragePatch> for proto::account::AccountStoragePatch {
    fn from(patch: &AccountStoragePatch) -> Self {
        Self {
            slots: patch
                .slots()
                .map(|(slot_name, slot_patch)| {
                    use proto::account::storage_slot_patch::Patch;

                    let patch = match slot_patch {
                        StorageSlotPatch::Value(value) => Patch::Value(value.into()),
                        StorageSlotPatch::Map(map) => Patch::Map(map.into()),
                    };
                    proto::account::StorageSlotPatch {
                        slot_name: slot_name.as_str().to_owned(),
                        patch: Some(patch),
                    }
                })
                .collect(),
        }
    }
}

impl TryFrom<proto::account::AccountStoragePatch> for AccountStoragePatch {
    type Error = ConversionError;

    fn try_from(patch: proto::account::AccountStoragePatch) -> Result<Self, Self::Error> {
        use proto::account::storage_slot_patch::Patch;

        let slots = patch
            .slots
            .into_iter()
            .enumerate()
            .map(|(index, slot)| {
                let slot_path = format!("slots[{index}]");
                let slot_name = StorageSlotName::new(slot.slot_name)
                    .map_err(ConversionError::from)
                    .context("slot_name")
                    .context(slot_path.clone())?;
                let patch = match slot.patch {
                    Some(Patch::Value(value)) => StorageSlotPatch::Value(
                        value.try_into().context("patch").context(slot_path.clone())?,
                    ),
                    Some(Patch::Map(map)) => StorageSlotPatch::Map(
                        map.try_into().context("patch").context(slot_path.clone())?,
                    ),
                    None => {
                        return Err(ConversionError::missing_field::<
                            proto::account::StorageSlotPatch,
                        >("patch")
                        .context(slot_path));
                    },
                };
                Ok((slot_name, patch))
            })
            .collect::<Result<Vec<_>, ConversionError>>()?;

        AccountStoragePatch::from_entries(slots)
            .map_err(ConversionError::new)
            .context("slots")
    }
}

// VAULT AND ACCOUNT PATCHES
// ================================================================================================

fn decode_account_patch_version(version: i32) -> Result<(), ConversionError> {
    match proto::account::AccountPatchVersion::try_from(version) {
        Ok(proto::account::AccountPatchVersion::V1) => Ok(()),
        Ok(proto::account::AccountPatchVersion::Unspecified) => {
            Err(ConversionError::message("account patch version is unspecified"))
        },
        Err(error) => Err(ConversionError::with_source(
            format!("unknown account patch version {version}"),
            error,
        )),
    }
}

impl From<&AccountVaultPatch> for proto::account::AccountVaultPatch {
    fn from(patch: &AccountVaultPatch) -> Self {
        Self {
            entries: patch
                .iter()
                .map(|(asset_id, value)| proto::account::AccountVaultPatchEntry {
                    asset_id: Some(asset_id.to_word().into()),
                    value: Some((*value).into()),
                })
                .collect(),
        }
    }
}

impl TryFrom<proto::account::AccountVaultPatch> for AccountVaultPatch {
    type Error = ConversionError;

    fn try_from(patch: proto::account::AccountVaultPatch) -> Result<Self, Self::Error> {
        let mut entries = BTreeMap::new();
        for (index, entry) in patch.entries.into_iter().enumerate() {
            let decoder = entry.decoder();
            let asset_id: Word =
                required!(decoder, entry.asset_id).context(format!("entries[{index}]"))?;
            let asset_id = AssetId::try_from(asset_id)
                .map_err(ConversionError::from)
                .context("asset_id")
                .context(format!("entries[{index}]"))?;
            let value = required!(decoder, entry.value).context(format!("entries[{index}]"))?;
            if entries.insert(asset_id, value).is_some() {
                return Err(ConversionError::message("duplicate vault asset ID")
                    .context(format!("entries[{index}].asset_id")));
            }
        }

        AccountVaultPatch::new(entries)
            .map_err(ConversionError::from)
            .context("entries")
    }
}

impl From<&AccountPatch> for proto::account::AccountPatch {
    fn from(patch: &AccountPatch) -> Self {
        Self {
            version: proto::account::AccountPatchVersion::V1 as i32,
            account_id: Some(patch.id().into()),
            storage: Some(patch.storage().into()),
            vault: Some(patch.vault().into()),
            code: patch.code().map(Into::into),
            final_nonce: patch.final_nonce().map(Into::into),
        }
    }
}

impl From<AccountPatch> for proto::account::AccountPatch {
    fn from(patch: AccountPatch) -> Self {
        Self::from(&patch)
    }
}

impl TryFrom<proto::account::AccountPatch> for AccountPatch {
    type Error = ConversionError;

    fn try_from(patch: proto::account::AccountPatch) -> Result<Self, Self::Error> {
        decode_account_patch_version(patch.version).context("version")?;

        let decoder = patch.decoder();
        let account_id = required!(decoder, patch.account_id)?;
        let storage = required!(decoder, patch.storage)?;
        let vault = required!(decoder, patch.vault)?;
        let code = patch.code.map(TryInto::try_into).transpose().context("code")?;
        let final_nonce =
            patch.final_nonce.map(TryInto::try_into).transpose().context("final_nonce")?;

        AccountPatch::new(account_id, storage, vault, code, final_nonce)
            .map_err(ConversionError::new)
    }
}

impl From<&AccountUpdateDetails> for proto::account::AccountUpdateDetails {
    fn from(details: &AccountUpdateDetails) -> Self {
        use proto::account::account_update_details::Update;

        let update = match details {
            AccountUpdateDetails::Private => {
                Update::Private(proto::account::PrivateAccountUpdate {})
            },
            AccountUpdateDetails::Public(patch) => Update::Public(patch.into()),
        };
        Self { update: Some(update) }
    }
}

impl From<AccountUpdateDetails> for proto::account::AccountUpdateDetails {
    fn from(details: AccountUpdateDetails) -> Self {
        Self::from(&details)
    }
}

impl TryFrom<proto::account::AccountUpdateDetails> for AccountUpdateDetails {
    type Error = ConversionError;

    fn try_from(details: proto::account::AccountUpdateDetails) -> Result<Self, Self::Error> {
        use proto::account::account_update_details::Update;

        match details.update {
            Some(Update::Private(_)) => Ok(AccountUpdateDetails::Private),
            Some(Update::Public(patch)) => {
                patch.try_into().map(AccountUpdateDetails::Public).context("public")
            },
            None => Err(ConversionError::missing_field::<proto::account::AccountUpdateDetails>(
                "update",
            )),
        }
    }
}
