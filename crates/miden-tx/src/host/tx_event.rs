use alloc::vec::Vec;

use miden_lib::transaction::{EventId, TransactionEventId};
use miden_objects::account::{AccountId, StorageMap};
use miden_objects::asset::{Asset, AssetVault, AssetVaultKey, FungibleAsset};
use miden_objects::note::{NoteInputs, NoteMetadata};
use miden_objects::{Felt, Word};
use miden_processor::{AdviceMutation, ProcessState};

use crate::TransactionKernelError;
use crate::auth::SigningInputs;
use crate::host::TransactionKernelProcess;

/// Indicates whether a [`TransactionEvent`] was handled or not.
///
/// If it is unhandled, the necessary data to handle it is returned.
#[derive(Debug)]
pub(crate) enum TransactionEventHandling {
    Unhandled(TransactionEvent),
    Handled(Vec<AdviceMutation>),
}

// TRANSACTION EVENT
// ================================================================================================

/// The data necessary to handle a [`TransactionEventId`].
#[derive(Debug, Clone)]
pub(crate) enum TransactionEvent {
    /// The data necessary to request a foreign account's data from the data store.
    AccountBeforeForeignLoad {
        /// The foreign account's ID.
        foreign_account_id: AccountId,
    },

    AccountVaultAfterRemoveAsset {
        asset: Asset,
    },

    AccountVaultAfterAddAsset {
        asset: Asset,
    },

    AccountStorageAfterSetItem {
        slot_idx: u8,
        current_value: Word,
        new_value: Word,
    },

    AccountStorageAfterSetMapItem {
        slot_index: u8,
        key: Word,
        prev_map_value: Word,
        new_map_value: Word,
    },

    /// The data necessary to request a storage map witness from the data store.
    AccountStorageBeforeMapItemAccess {
        /// The account ID for whose storage a witness is requested.
        active_account_id: AccountId,
        /// The slot index of the map.
        slot_index: u8,
        /// The root of the storage map in the account at the beginning of the transaction.
        current_map_root: Word,
        /// The raw map key for which a witness is requested.
        map_key: Word,
        /// Indicates whether the witness for this map item is already in the advice provider.
        is_witness_present: bool,
    },

    /// The data necessary to request an asset witness from the data store.
    AccountVaultBeforeAssetAccess {
        /// The account ID for whose vault a witness is requested.
        active_account_id: AccountId,
        /// The vault root identifying the asset vault from which a witness is requested.
        current_vault_root: Word,
        /// The asset for which a witness is requested.
        asset_key: AssetVaultKey,
        /// Indicates whether the witness for this map item is already in the advice provider.
        is_witness_present: bool,
    },

    /// The data necessary to handle an auth request.
    AuthRequest {
        /// The hash of the public key for which a signature was requested.
        pub_key_hash: Word,
        /// The signing inputs that summarize what is being signed. The commitment to these inputs
        /// is the message that is being signed.
        signing_inputs: SigningInputs,
    },
    /// The data necessary to handle a transaction fee computed event.
    TransactionFeeComputed {
        /// The fee asset extracted from the stack.
        fee_asset: FungibleAsset,
    },

    /// The data necessary to request a note script from the data store.
    NoteData {
        /// The note index extracted from the stack.
        note_idx: usize,
        /// The note metadata extracted from the stack.
        metadata: NoteMetadata,
        /// The root of the note script being requested.
        script_root: Word,
        /// The recipient digest extracted from the stack.
        recipient_digest: Word,
        /// The note inputs extracted from the advice provider.
        note_inputs: NoteInputs,
        /// The serial number extracted from the advice provider.
        serial_num: Word,
    },
}

impl TransactionEvent {
    pub fn extract_from_process(
        process: &ProcessState,
    ) -> Result<Option<TransactionEvent>, TransactionKernelError> {
        let event_id = EventId::from_felt(process.get_stack_item(0));
        let tx_event_id = TransactionEventId::try_from(event_id).expect("TODO");

        let tx_event = match tx_event_id {
            TransactionEventId::AccountBeforeForeignLoad => {
                // Expected stack state: `[event, account_id_prefix, account_id_suffix]`
                let account_id_word = process.get_stack_word_be(1);
                let account_id = AccountId::try_from([account_id_word[3], account_id_word[2]])
                    .map_err(|err| {
                        TransactionKernelError::other_with_source(
                            "failed to convert account ID word into account ID",
                            err,
                        )
                    })?;

                TransactionEvent::AccountBeforeForeignLoad { foreign_account_id: account_id }
            },
            TransactionEventId::AccountVaultBeforeAddAsset
            | TransactionEventId::AccountVaultBeforeRemoveAsset => {
                // Expected stack state: `[event, ASSET, account_vault_root_ptr]`
                let asset_word = process.get_stack_word_be(1);
                let asset = Asset::try_from(asset_word).map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_before_add_or_remove_asset",
                        source,
                    }
                })?;

                let vault_root_ptr = process.get_stack_item(5);
                let vault_root_ptr = u32::try_from(vault_root_ptr).map_err(|_err| {
                    TransactionKernelError::other(format!(
                        "vault root ptr should fit into a u32, but was {vault_root_ptr}"
                    ))
                })?;
                let current_vault_root = process
                    .get_mem_word(process.ctx(), vault_root_ptr)
                    .map_err(|_err| {
                        TransactionKernelError::other(format!(
                            "vault root ptr {vault_root_ptr} is not word-aligned"
                        ))
                    })?
                    .ok_or_else(|| {
                        TransactionKernelError::other(format!(
                            "vault root ptr {vault_root_ptr} was not initialized"
                        ))
                    })?;

                Self::on_account_vault_asset_accessed(
                    process,
                    asset.vault_key(),
                    current_vault_root,
                )?
            },
            TransactionEventId::AccountVaultAfterRemoveAsset => {
                // Expected stack state: `[event, ASSET]`
                let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_after_remove_asset",
                        source,
                    }
                })?;

                TransactionEvent::AccountVaultAfterRemoveAsset { asset }
            },
            TransactionEventId::AccountVaultAfterAddAsset => {
                // Expected stack state: `[event, ASSET]`
                let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_after_add_asset",
                        source,
                    }
                })?;

                TransactionEvent::AccountVaultAfterAddAsset { asset }
            },
            TransactionEventId::AccountVaultBeforeGetBalance => {
                // Expected stack state:
                // [event, faucet_id_prefix, faucet_id_suffix, vault_root_ptr]
                let stack_top = process.get_stack_word_be(1);
                let faucet_id =
                    AccountId::try_from([stack_top[3], stack_top[2]]).map_err(|err| {
                        TransactionKernelError::other_with_source(
                            "failed to convert faucet ID word into faucet ID",
                            err,
                        )
                    })?;
                let vault_root_ptr = stack_top[1];
                let vault_root = process.get_vault_root(vault_root_ptr)?;

                let vault_key = AssetVaultKey::from_account_id(faucet_id).ok_or_else(|| {
                    TransactionKernelError::other(format!(
                        "provided faucet ID {faucet_id} is not valid for fungible assets"
                    ))
                })?;

                Self::on_account_vault_asset_accessed(process, vault_key, vault_root)?
            },
            TransactionEventId::AccountVaultBeforeHasNonFungibleAsset => {
                // Expected stack state: `[event, ASSET, vault_root_ptr]`
                let asset_word = process.get_stack_word_be(1);
                let asset = Asset::try_from(asset_word).map_err(|err| {
                    TransactionKernelError::other_with_source(
                        "provided asset is not a valid asset",
                        err,
                    )
                })?;

                let vault_root_ptr = process.get_stack_item(5);
                let vault_root = process.get_vault_root(vault_root_ptr)?;

                Self::on_account_vault_asset_accessed(process, asset.vault_key(), vault_root)?
            },
            TransactionEventId::AccountStorageAfterSetItem => {
                // Expected stack state:
                // [event, slot_index, NEW_SLOT_VALUE, CURRENT_SLOT_VALUE]

                // get slot index from the stack and make sure it is valid
                let slot_index = process.get_stack_item(1);
                let slot_index = u8::try_from(slot_index).map_err(|err| {
                    TransactionKernelError::other(format!(
                        "failed to convert slot index into u8: {err}"
                    ))
                })?;

                // get number of storage slots initialized by the account
                let num_storage_slot = process.get_num_storage_slots()?;
                if slot_index as u64 >= num_storage_slot {
                    return Err(TransactionKernelError::InvalidStorageSlotIndex {
                        max: num_storage_slot,
                        actual: slot_index as u64,
                    });
                }

                // get the value to which the slot is being updated
                let new_slot_value = process.get_stack_word_be(2);

                // get the current value for the slot
                let current_slot_value = process.get_stack_word_be(6);

                TransactionEvent::AccountStorageAfterSetItem {
                    slot_idx: slot_index,
                    current_value: current_slot_value,
                    new_value: new_slot_value,
                }
            },

            TransactionEventId::AccountStorageBeforeGetMapItem => {
                // Expected stack state: [event, KEY, ROOT, index]

                let map_key = process.get_stack_word_be(1);
                let current_map_root = process.get_stack_word_be(5);
                let slot_index = process.get_stack_item(9);

                Self::on_account_storage_map_item_accessed(
                    process,
                    slot_index,
                    current_map_root,
                    map_key,
                )?
            },

            TransactionEventId::AccountStorageBeforeSetMapItem => {
                // Expected stack state: [event, index, KEY, NEW_VALUE, OLD_ROOT]
                let slot_index = process.get_stack_item(1);
                let map_key = process.get_stack_word_be(2);
                let current_map_root = process.get_stack_word_be(10);

                Self::on_account_storage_map_item_accessed(
                    process,
                    slot_index,
                    current_map_root,
                    map_key,
                )?
            },

            TransactionEventId::AccountStorageAfterSetMapItem => {
                // Expected stack state: [event, slot_index, KEY, PREV_MAP_VALUE, NEW_MAP_VALUE]

                // get slot index from the stack and make sure it is valid
                let slot_index = process.get_stack_item(1);
                let slot_index = u8::try_from(slot_index).map_err(|err| {
                    TransactionKernelError::other(format!(
                        "failed to convert slot index into u8: {err}"
                    ))
                })?;

                // get number of storage slots initialized by the account
                let num_storage_slot = process.get_num_storage_slots()?;
                if slot_index as u64 >= num_storage_slot {
                    return Err(TransactionKernelError::InvalidStorageSlotIndex {
                        max: num_storage_slot,
                        actual: slot_index as u64,
                    });
                }

                // get the KEY to which the slot is being updated
                let key = process.get_stack_word_be(2);

                // get the previous VALUE of the slot
                let prev_map_value = process.get_stack_word_be(6);

                // get the VALUE to which the slot is being updated
                let new_map_value = process.get_stack_word_be(10);

                TransactionEvent::AccountStorageAfterSetMapItem {
                    slot_index,
                    key,
                    prev_map_value,
                    new_map_value,
                }
            },

            // Events that do not require handlers.
            TransactionEventId::AccountBeforeIncrementNonce => {
                return Ok(None);
            },

            _ => unimplemented!(),
        };

        Ok(Some(tx_event))
    }

    /// Checks if the necessary witness for accessing the provided asset is already in the merkle
    /// store, and if not, extracts all necessary data for requesting it.
    fn on_account_vault_asset_accessed(
        process: &ProcessState,
        vault_key: AssetVaultKey,
        current_vault_root: Word,
    ) -> Result<TransactionEvent, TransactionKernelError> {
        let leaf_index = Felt::new(vault_key.to_leaf_index().value());
        let active_account_id = process.get_active_account_id()?;

        // Note that we check whether a merkle path for the current vault root is present, not
        // necessarily for the root we are going to request. This is because the end goal is to
        // enable access to an asset against the current vault root, and so if this
        // condition is already satisfied, there is nothing to request.
        let is_witness_present = advice_provider_has_merkle_path::<{ AssetVault::DEPTH }>(
            process,
            current_vault_root,
            leaf_index,
        )?;

        Ok(TransactionEvent::AccountVaultBeforeAssetAccess {
            active_account_id,
            current_vault_root,
            asset_key: vault_key,
            is_witness_present,
        })
    }

    /// Checks if the necessary witness for accessing the map item is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    fn on_account_storage_map_item_accessed(
        process: &ProcessState,
        slot_index: Felt,
        current_map_root: Word,
        map_key: Word,
    ) -> Result<TransactionEvent, TransactionKernelError> {
        let active_account_id = process.get_active_account_id()?;
        let hashed_map_key = StorageMap::hash_key(map_key);
        let leaf_index = StorageMap::hashed_map_key_to_leaf_index(hashed_map_key);

        let slot_index = u8::try_from(slot_index).map_err(|err| {
            TransactionKernelError::other(format!("failed to convert slot index into u8: {err}"))
        })?;

        let is_witness_present = advice_provider_has_merkle_path::<{ AssetVault::DEPTH }>(
            process,
            current_map_root,
            leaf_index,
        )?;

        Ok(TransactionEvent::AccountStorageBeforeMapItemAccess {
            active_account_id,
            slot_index,
            current_map_root,
            map_key,
            is_witness_present,
        })
    }
}

/// Returns `true` if the advice provider has a merkle path for the provided root and leaf
/// index, `false` otherwise.
fn advice_provider_has_merkle_path<const TREE_DEPTH: u8>(
    process: &ProcessState,
    root: Word,
    leaf_index: Felt,
) -> Result<bool, TransactionKernelError> {
    process
        .advice_provider()
        .has_merkle_path(root, Felt::from(TREE_DEPTH), leaf_index)
        .map_err(|err| {
            TransactionKernelError::other_with_source(
                "failed to check for merkle path presence in advice provider",
                err,
            )
        })
}
