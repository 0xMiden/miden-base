use alloc::vec::Vec;

use miden_lib::transaction::{EventId, TransactionEventId};
use miden_objects::account::{AccountId, StorageMap};
use miden_objects::asset::{Asset, AssetVault, AssetVaultKey, FungibleAsset};
use miden_objects::note::{NoteId, NoteInputs, NoteMetadata, NoteScript};
use miden_objects::transaction::TransactionSummary;
use miden_objects::{Felt, Hasher, Word};
use miden_processor::{AdviceMutation, ProcessState, RowIndex};

use crate::host::{TransactionBaseHost, TransactionKernelProcess};
use crate::{LinkMap, TransactionKernelError};

// TRANSACTION EVENT
// ================================================================================================

/// The data necessary to handle a [`TransactionEventId`].
#[derive(Debug)]
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
    },

    /// The data necessary to request an asset witness from the data store.
    AccountVaultBeforeAssetAccess {
        /// The account ID for whose vault a witness is requested.
        active_account_id: AccountId,
        /// The vault root identifying the asset vault from which a witness is requested.
        current_vault_root: Word,
        /// The asset for which a witness is requested.
        asset_key: AssetVaultKey,
    },

    AccountAfterIncrementNonce,

    AccountPushProcedureIndex {
        /// The code commitment of the active account.
        code_commitment: Word,
        /// The procedure root whose index is requested.
        procedure_root: Word,
    },

    NoteAfterCreated {
        /// The note index extracted from the stack.
        note_idx: usize,
        /// The note metadata extracted from the stack.
        metadata: NoteMetadata,
        /// The recipient digest extracted from the stack.
        recipient_digest: Word,
        /// The note script of the note being created, which is present if it existed in the advice
        /// provider.
        note_script: Option<NoteScript>,
        /// The recipient data extracted from the advice inputs.
        recipient_data: Option<RecipientData>,
    },

    NoteBeforeAddAsset {
        /// The note index to which the asset is added.
        note_idx: usize,
        /// The asset that is added to the output note.
        asset: Asset,
    },

    /// The data necessary to handle an auth request.
    AuthRequest {
        pub_key_hash: Word,
        tx_summary: TransactionSummary,
        signature: Option<Vec<Felt>>,
    },

    Unauthorized {
        tx_summary: TransactionSummary,
    },

    EpilogueBeforeTxFeeRemovedFromAccount {
        fee_asset: FungibleAsset,
    },

    LinkMapSet {
        advice_mutation: Vec<AdviceMutation>,
    },
    LinkMapGet {
        advice_mutation: Vec<AdviceMutation>,
    },

    PrologueStart {
        clk: RowIndex,
    },
    PrologueEnd {
        clk: RowIndex,
    },

    NotesProcessingStart {
        clk: RowIndex,
    },
    NotesProcessingEnd {
        clk: RowIndex,
    },

    NoteExecutionStart {
        note_id: NoteId,
        clk: RowIndex,
    },
    NoteExecutionEnd {
        clk: RowIndex,
    },

    TxScriptProcessingStart {
        clk: RowIndex,
    },
    TxScriptProcessingEnd {
        clk: RowIndex,
    },

    EpilogueStart {
        clk: RowIndex,
    },
    EpilogueEnd {
        clk: RowIndex,
    },

    EpilogueAuthProcStart {
        clk: RowIndex,
    },
    EpilogueAuthProcEnd {
        clk: RowIndex,
    },

    EpilogueAfterTxCyclesObtained {
        clk: RowIndex,
    },
}

impl TransactionEvent {
    /// Extracts the [`TransactionEventId`] from the stack as well as the data necessary to handle
    /// it.
    ///
    /// Returns `Some` if the extracted [`TransactionEventId`] resulted in an event that needs to be
    /// handled, `None` otherwise.
    pub fn extract<'store, STORE>(
        base_host: &TransactionBaseHost<'store, STORE>,
        process: &ProcessState,
    ) -> Result<Option<TransactionEvent>, TransactionKernelError> {
        let event_id = EventId::from_felt(process.get_stack_item(0));
        let tx_event_id = TransactionEventId::try_from(event_id).map_err(|err| {
            TransactionKernelError::other_with_source(
                "failed to convert event ID into transaction event ID",
                err,
            )
        })?;

        let tx_event = match tx_event_id {
            TransactionEventId::AccountBeforeForeignLoad => {
                // Expected stack state: [event, account_id_prefix, account_id_suffix]
                let account_id_word = process.get_stack_word_be(1);
                let account_id = AccountId::try_from([account_id_word[3], account_id_word[2]])
                    .map_err(|err| {
                        TransactionKernelError::other_with_source(
                            "failed to convert account ID word into account ID",
                            err,
                        )
                    })?;

                Some(TransactionEvent::AccountBeforeForeignLoad { foreign_account_id: account_id })
            },
            TransactionEventId::AccountVaultBeforeAddAsset
            | TransactionEventId::AccountVaultBeforeRemoveAsset => {
                // Expected stack state: [event, ASSET, account_vault_root_ptr]
                let asset_word = process.get_stack_word_be(1);
                let asset = Asset::try_from(asset_word).map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_before_add_or_remove_asset",
                        source,
                    }
                })?;

                let vault_root_ptr = process.get_stack_item(5);
                let current_vault_root = process.get_vault_root(vault_root_ptr)?;

                Self::on_account_vault_asset_accessed(
                    process,
                    asset.vault_key(),
                    current_vault_root,
                )?
            },
            TransactionEventId::AccountVaultAfterRemoveAsset => {
                // Expected stack state: [event, ASSET]
                let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_after_remove_asset",
                        source,
                    }
                })?;

                Some(TransactionEvent::AccountVaultAfterRemoveAsset { asset })
            },
            TransactionEventId::AccountVaultAfterAddAsset => {
                // Expected stack state: [event, ASSET]
                let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_account_vault_after_add_asset",
                        source,
                    }
                })?;

                Some(TransactionEvent::AccountVaultAfterAddAsset { asset })
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
                // Expected stack state: [event, ASSET, vault_root_ptr]
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

            TransactionEventId::AccountStorageBeforeSetItem => None,

            TransactionEventId::AccountStorageAfterSetItem => {
                // Expected stack state:
                // [event, slot_index, NEW_SLOT_VALUE]
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

                Some(TransactionEvent::AccountStorageAfterSetItem {
                    slot_idx: slot_index,
                    new_value: new_slot_value,
                })
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

                Some(TransactionEvent::AccountStorageAfterSetMapItem {
                    slot_index,
                    key,
                    prev_map_value,
                    new_map_value,
                })
            },

            TransactionEventId::AccountBeforeIncrementNonce => None,

            TransactionEventId::AccountAfterIncrementNonce => {
                Some(TransactionEvent::AccountAfterIncrementNonce)
            },

            TransactionEventId::AccountPushProcedureIndex => {
                // Expected stack state: [event, PROC_ROOT]
                let procedure_root = process.get_stack_word_be(1);
                let code_commitment = process.get_active_account_code_commitment()?;

                Some(TransactionEvent::AccountPushProcedureIndex {
                    code_commitment,
                    procedure_root,
                })
            },

            TransactionEventId::NoteBeforeCreated => None,

            TransactionEventId::NoteAfterCreated => {
                // Expected stack state: [event, NOTE_METADATA, note_ptr, RECIPIENT, note_idx]
                let metadata_word = process.get_stack_word_be(1);
                let metadata = NoteMetadata::try_from(metadata_word)
                    .map_err(TransactionKernelError::MalformedNoteMetadata)?;

                let recipient_digest = process.get_stack_word_be(6);
                let note_idx = process.get_stack_item(10).as_int() as usize;

                // try to read the full recipient from the advice provider
                let (note_script, recipient_data) =
                    if process.has_advice_map_entry(recipient_digest) {
                        let (note_inputs, script_root, serial_num) =
                            process.read_note_recipient_info_from_adv_map(recipient_digest)?;

                        let note_script = process
                            .advice_provider()
                            .get_mapped_values(&script_root)
                            .map(|script_data| {
                                NoteScript::try_from(script_data).map_err(|source| {
                                    TransactionKernelError::MalformedNoteScript {
                                        data: script_data.to_vec(),
                                        source,
                                    }
                                })
                            })
                            .transpose()?;

                        (note_script, Some(RecipientData { serial_num, script_root, note_inputs }))
                    } else {
                        (None, None)
                    };

                Some(TransactionEvent::NoteAfterCreated {
                    note_idx,
                    metadata,
                    recipient_digest,
                    note_script,
                    recipient_data,
                })
            },

            TransactionEventId::NoteBeforeAddAsset => {
                // Expected stack state: [event, ASSET, note_ptr, num_of_assets, note_idx]
                let note_idx = process.get_stack_item(7).as_int() as usize;

                let asset_word = process.get_stack_word_be(1);
                let asset = Asset::try_from(asset_word).map_err(|source| {
                    TransactionKernelError::MalformedAssetInEventHandler {
                        handler: "on_note_before_add_asset",
                        source,
                    }
                })?;

                Some(TransactionEvent::NoteBeforeAddAsset { note_idx, asset })
            },

            TransactionEventId::NoteAfterAddAsset => None,

            TransactionEventId::AuthRequest => {
                // Expected stack state: [event, MESSAGE, PUB_KEY]
                let message = process.get_stack_word_be(1);
                let pub_key_hash = process.get_stack_word_be(5);
                let signature_key = Hasher::merge(&[pub_key_hash, message]);

                let signature = process
                    .advice_provider()
                    .get_mapped_values(&signature_key)
                    .map(|slice| slice.to_vec());

                let tx_summary = Self::extract_tx_summary(base_host, process, message)?;

                Some(TransactionEvent::AuthRequest { pub_key_hash, tx_summary, signature })
            },

            TransactionEventId::Unauthorized => {
                // Expected stack state: [event, MESSAGE]
                let message = process.get_stack_word_be(1);
                let tx_summary = Self::extract_tx_summary(base_host, process, message)?;

                Some(TransactionEvent::Unauthorized { tx_summary })
            },

            TransactionEventId::EpilogueBeforeTxFeeRemovedFromAccount => {
                // Expected stack state: [event, FEE_ASSET]
                let fee_asset = process.get_stack_word_be(1);
                let fee_asset = FungibleAsset::try_from(fee_asset)
                    .map_err(TransactionKernelError::FailedToConvertFeeAsset)?;

                Some(TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount { fee_asset })
            },

            TransactionEventId::LinkMapSet => Some(TransactionEvent::LinkMapSet {
                advice_mutation: LinkMap::handle_set_event(process),
            }),
            TransactionEventId::LinkMapGet => Some(TransactionEvent::LinkMapGet {
                advice_mutation: LinkMap::handle_get_event(process),
            }),

            TransactionEventId::PrologueStart => {
                Some(TransactionEvent::PrologueStart { clk: process.clk() })
            },
            TransactionEventId::PrologueEnd => {
                Some(TransactionEvent::PrologueEnd { clk: process.clk() })
            },

            TransactionEventId::NotesProcessingStart => {
                Some(TransactionEvent::NotesProcessingStart { clk: process.clk() })
            },
            TransactionEventId::NotesProcessingEnd => {
                Some(TransactionEvent::NotesProcessingEnd { clk: process.clk() })
            },

            TransactionEventId::NoteExecutionStart => {
                let note_id = process.get_active_note_id()?.ok_or_else(|| TransactionKernelError::other(
                    "note execution interval measurement is incorrect: check the placement of the start and the end of the interval",
                ))?;

                Some(TransactionEvent::NoteExecutionStart { note_id, clk: process.clk() })
            },
            TransactionEventId::NoteExecutionEnd => {
                Some(TransactionEvent::NoteExecutionEnd { clk: process.clk() })
            },

            TransactionEventId::TxScriptProcessingStart => {
                Some(TransactionEvent::TxScriptProcessingStart { clk: process.clk() })
            },
            TransactionEventId::TxScriptProcessingEnd => {
                Some(TransactionEvent::TxScriptProcessingEnd { clk: process.clk() })
            },

            TransactionEventId::EpilogueStart => {
                Some(TransactionEvent::EpilogueStart { clk: process.clk() })
            },
            TransactionEventId::EpilogueEnd => {
                Some(TransactionEvent::EpilogueEnd { clk: process.clk() })
            },

            TransactionEventId::EpilogueAuthProcStart => {
                Some(TransactionEvent::EpilogueAuthProcStart { clk: process.clk() })
            },
            TransactionEventId::EpilogueAuthProcEnd => {
                Some(TransactionEvent::EpilogueAuthProcEnd { clk: process.clk() })
            },

            TransactionEventId::EpilogueAfterTxCyclesObtained => {
                Some(TransactionEvent::EpilogueAfterTxCyclesObtained { clk: process.clk() })
            },
        };

        Ok(tx_event)
    }

    /// Checks if the necessary witness for accessing the asset is already in the merkle store, and
    /// extracts all necessary data for requesting it.
    fn on_account_vault_asset_accessed(
        process: &ProcessState,
        vault_key: AssetVaultKey,
        current_vault_root: Word,
    ) -> Result<Option<TransactionEvent>, TransactionKernelError> {
        let leaf_index = Felt::new(vault_key.to_leaf_index().value());
        let active_account_id = process.get_active_account_id()?;

        // Note that we check whether a merkle path for the current vault root is present, not
        // necessarily for the root we are going to request. This is because the end goal is to
        // enable access to an asset against the current vault root, and so if this
        // condition is already satisfied, there is nothing to request.
        if process.has_merkle_path::<{ AssetVault::DEPTH }>(current_vault_root, leaf_index)? {
            // If the witness already exists, the event does not need to be handled.
            Ok(None)
        } else {
            Ok(Some(TransactionEvent::AccountVaultBeforeAssetAccess {
                active_account_id,
                current_vault_root,
                asset_key: vault_key,
            }))
        }
    }

    /// Checks if the necessary witness for accessing the map item is already in the merkle store,
    /// and extracts all necessary data for requesting it.
    fn on_account_storage_map_item_accessed(
        process: &ProcessState,
        slot_index: Felt,
        current_map_root: Word,
        map_key: Word,
    ) -> Result<Option<TransactionEvent>, TransactionKernelError> {
        let active_account_id = process.get_active_account_id()?;
        let hashed_map_key = StorageMap::hash_key(map_key);
        let leaf_index = StorageMap::hashed_map_key_to_leaf_index(hashed_map_key);

        let slot_index = u8::try_from(slot_index).map_err(|err| {
            TransactionKernelError::other(format!("failed to convert slot index into u8: {err}"))
        })?;

        if process.has_merkle_path::<{ StorageMap::DEPTH }>(current_map_root, leaf_index)? {
            // If the witness already exists, the event does not need to be handled.
            Ok(None)
        } else {
            Ok(Some(TransactionEvent::AccountStorageBeforeMapItemAccess {
                active_account_id,
                slot_index,
                current_map_root,
                map_key,
            }))
        }
    }

    /// Extracts the transaction summary from the advice map using the provided `message` as the
    /// key.
    ///
    /// ```text
    /// Expected advice map state: {
    ///     MESSAGE: [
    ///         SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT
    ///     ]
    /// }
    /// ```
    fn extract_tx_summary<'store, STORE>(
        base_host: &TransactionBaseHost<'store, STORE>,
        process: &ProcessState,
        message: Word,
    ) -> Result<TransactionSummary, TransactionKernelError> {
        let Some(commitments) = process.advice_provider().get_mapped_values(&message) else {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "expected message to exist in advice provider".into(),
            ));
        };

        if commitments.len() != 16 {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "expected 4 words for transaction summary commitments".into(),
            ));
        }

        let salt = extract_word(commitments, 0);
        let output_notes_commitment = extract_word(commitments, 4);
        let input_notes_commitment = extract_word(commitments, 8);
        let account_delta_commitment = extract_word(commitments, 12);

        let tx_summary = base_host.build_tx_summary(
            salt,
            output_notes_commitment,
            input_notes_commitment,
            account_delta_commitment,
        )?;

        if tx_summary.to_commitment() != message {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "transaction summary doesn't commit to the expected message".into(),
            ));
        }

        Ok(tx_summary)
    }
}

// RECIPIENT DATA
// ================================================================================================

/// The partial data to construct a note recipient.
#[derive(Debug)]
pub(crate) struct RecipientData {
    pub(crate) serial_num: Word,
    pub(crate) script_root: Word,
    pub(crate) note_inputs: NoteInputs,
}

// HELPER FUNCTIONS
// ================================================================================================

/// Extracts a word from a slice of field elements.
#[inline(always)]
fn extract_word(commitments: &[Felt], start: usize) -> Word {
    Word::from([
        commitments[start],
        commitments[start + 1],
        commitments[start + 2],
        commitments[start + 3],
    ])
}
