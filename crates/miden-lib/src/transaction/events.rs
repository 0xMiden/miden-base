use core::fmt;

pub use miden_core::EventId;

use super::TransactionEventError;

// CONSTANTS
// ================================================================================================
// Include the generated event constants
include!(concat!(env!("OUT_DIR"), "/assets/transaction_events.rs"));

// TRANSACTION EVENT
// ================================================================================================

/// Events which may be emitted by a transaction kernel.
///
/// The events are emitted via the `emit.<event_id>` instruction. The event ID is a Felt
/// derived from the `EventId` string which is used to identify the event type. Events emitted
/// by the transaction kernel are in the `miden` namespace.
#[repr(u64)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TransactionEvent {
    AccountBeforeForeignLoad = ACCOUNT_BEFORE_FOREIGN_LOAD,

    AccountVaultBeforeAddAsset = ACCOUNT_VAULT_BEFORE_ADD_ASSET,
    AccountVaultAfterAddAsset = ACCOUNT_VAULT_AFTER_ADD_ASSET,

    AccountVaultBeforeRemoveAsset = ACCOUNT_VAULT_BEFORE_REMOVE_ASSET,
    AccountVaultAfterRemoveAsset = ACCOUNT_VAULT_AFTER_REMOVE_ASSET,

    AccountVaultBeforeGetBalance = ACCOUNT_VAULT_BEFORE_GET_BALANCE,

    AccountVaultBeforeHasNonFungibleAsset = ACCOUNT_VAULT_BEFORE_HAS_NON_FUNGIBLE_ASSET,

    AccountStorageBeforeSetItem = ACCOUNT_STORAGE_BEFORE_SET_ITEM,
    AccountStorageAfterSetItem = ACCOUNT_STORAGE_AFTER_SET_ITEM,

    AccountStorageBeforeGetMapItem = ACCOUNT_STORAGE_BEFORE_GET_MAP_ITEM,

    AccountStorageBeforeSetMapItem = ACCOUNT_STORAGE_BEFORE_SET_MAP_ITEM,
    AccountStorageAfterSetMapItem = ACCOUNT_STORAGE_AFTER_SET_MAP_ITEM,

    AccountBeforeIncrementNonce = ACCOUNT_BEFORE_INCREMENT_NONCE,
    AccountAfterIncrementNonce = ACCOUNT_AFTER_INCREMENT_NONCE,

    AccountPushProcedureIndex = ACCOUNT_PUSH_PROCEDURE_INDEX,

    NoteBeforeCreated = NOTE_BEFORE_CREATED,
    NoteAfterCreated = NOTE_AFTER_CREATED,

    NoteBeforeAddAsset = NOTE_BEFORE_ADD_ASSET,
    NoteAfterAddAsset = NOTE_AFTER_ADD_ASSET,

    AuthRequest = AUTH_REQUEST,

    PrologueStart = PROLOGUE_START,
    PrologueEnd = PROLOGUE_END,

    NotesProcessingStart = NOTES_PROCESSING_START,
    NotesProcessingEnd = NOTES_PROCESSING_END,

    NoteExecutionStart = NOTE_EXECUTION_START,
    NoteExecutionEnd = NOTE_EXECUTION_END,

    TxScriptProcessingStart = TX_SCRIPT_PROCESSING_START,
    TxScriptProcessingEnd = TX_SCRIPT_PROCESSING_END,

    EpilogueStart = EPILOGUE_START,
    EpilogueEnd = EPILOGUE_END,

    EpilogueAuthProcStart = EPILOGUE_AUTH_PROC_START,
    EpilogueAuthProcEnd = EPILOGUE_AUTH_PROC_END,

    EpilogueAfterTxCyclesObtained = EPILOGUE_AFTER_TX_CYCLES_OBTAINED,
    EpilogueBeforeTxFeeRemovedFromAccount = EPILOGUE_BEFORE_TX_FEE_REMOVED_FROM_ACCOUNT,

    LinkMapSet = LINK_MAP_SET,
    LinkMapGet = LINK_MAP_GET,

    Unauthorized = AUTH_UNAUTHORIZED,
}

impl TransactionEvent {
    /// Returns `true` if the event is privileged, i.e. it is only allowed to be emitted from the
    /// root context of the VM, which is where the transaction kernel executes.
    pub fn is_privileged(&self) -> bool {
        let is_unprivileged = matches!(self, Self::AuthRequest | Self::Unauthorized);
        !is_unprivileged
    }

    /// Returns the [`EventId`] of the transaction event.
    pub fn event_id(&self) -> EventId {
        EventId::from_u64(self.clone() as u64)
    }
}

impl fmt::Display for TransactionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl TryFrom<EventId> for TransactionEvent {
    type Error = TransactionEventError;

    fn try_from(event_id: EventId) -> Result<Self, Self::Error> {
        let raw = event_id.as_felt().as_int();

        let name = EVENT_NAME_LUT.get(&raw).copied();

        match raw {
            ACCOUNT_BEFORE_FOREIGN_LOAD => Ok(TransactionEvent::AccountBeforeForeignLoad),

            ACCOUNT_VAULT_BEFORE_ADD_ASSET => Ok(TransactionEvent::AccountVaultBeforeAddAsset),
            ACCOUNT_VAULT_AFTER_ADD_ASSET => Ok(TransactionEvent::AccountVaultAfterAddAsset),

            ACCOUNT_VAULT_BEFORE_REMOVE_ASSET => {
                Ok(TransactionEvent::AccountVaultBeforeRemoveAsset)
            },
            ACCOUNT_VAULT_AFTER_REMOVE_ASSET => Ok(TransactionEvent::AccountVaultAfterRemoveAsset),

            ACCOUNT_VAULT_BEFORE_GET_BALANCE => Ok(TransactionEvent::AccountVaultBeforeGetBalance),

            ACCOUNT_VAULT_BEFORE_HAS_NON_FUNGIBLE_ASSET => {
                Ok(TransactionEvent::AccountVaultBeforeHasNonFungibleAsset)
            },

            ACCOUNT_STORAGE_BEFORE_SET_ITEM => Ok(TransactionEvent::AccountStorageBeforeSetItem),
            ACCOUNT_STORAGE_AFTER_SET_ITEM => Ok(TransactionEvent::AccountStorageAfterSetItem),

            ACCOUNT_STORAGE_BEFORE_GET_MAP_ITEM => {
                Ok(TransactionEvent::AccountStorageBeforeGetMapItem)
            },

            ACCOUNT_STORAGE_BEFORE_SET_MAP_ITEM => {
                Ok(TransactionEvent::AccountStorageBeforeSetMapItem)
            },
            ACCOUNT_STORAGE_AFTER_SET_MAP_ITEM => {
                Ok(TransactionEvent::AccountStorageAfterSetMapItem)
            },

            ACCOUNT_BEFORE_INCREMENT_NONCE => Ok(TransactionEvent::AccountBeforeIncrementNonce),
            ACCOUNT_AFTER_INCREMENT_NONCE => Ok(TransactionEvent::AccountAfterIncrementNonce),

            ACCOUNT_PUSH_PROCEDURE_INDEX => Ok(TransactionEvent::AccountPushProcedureIndex),

            NOTE_BEFORE_CREATED => Ok(TransactionEvent::NoteBeforeCreated),
            NOTE_AFTER_CREATED => Ok(TransactionEvent::NoteAfterCreated),

            NOTE_BEFORE_ADD_ASSET => Ok(TransactionEvent::NoteBeforeAddAsset),
            NOTE_AFTER_ADD_ASSET => Ok(TransactionEvent::NoteAfterAddAsset),

            AUTH_REQUEST => Ok(TransactionEvent::AuthRequest),

            PROLOGUE_START => Ok(TransactionEvent::PrologueStart),
            PROLOGUE_END => Ok(TransactionEvent::PrologueEnd),

            NOTES_PROCESSING_START => Ok(TransactionEvent::NotesProcessingStart),
            NOTES_PROCESSING_END => Ok(TransactionEvent::NotesProcessingEnd),

            NOTE_EXECUTION_START => Ok(TransactionEvent::NoteExecutionStart),
            NOTE_EXECUTION_END => Ok(TransactionEvent::NoteExecutionEnd),

            TX_SCRIPT_PROCESSING_START => Ok(TransactionEvent::TxScriptProcessingStart),
            TX_SCRIPT_PROCESSING_END => Ok(TransactionEvent::TxScriptProcessingEnd),

            EPILOGUE_START => Ok(TransactionEvent::EpilogueStart),
            EPILOGUE_AUTH_PROC_START => Ok(TransactionEvent::EpilogueAuthProcStart),
            EPILOGUE_AUTH_PROC_END => Ok(TransactionEvent::EpilogueAuthProcEnd),
            EPILOGUE_AFTER_TX_CYCLES_OBTAINED => {
                Ok(TransactionEvent::EpilogueAfterTxCyclesObtained)
            },
            EPILOGUE_BEFORE_TX_FEE_REMOVED_FROM_ACCOUNT => {
                Ok(TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount)
            },
            EPILOGUE_END => Ok(TransactionEvent::EpilogueEnd),

            LINK_MAP_SET => Ok(TransactionEvent::LinkMapSet),
            LINK_MAP_GET => Ok(TransactionEvent::LinkMapGet),

            AUTH_UNAUTHORIZED => Ok(TransactionEvent::Unauthorized),

            _ => Err(TransactionEventError::InvalidTransactionEvent(event_id, name)),
        }
    }
}
