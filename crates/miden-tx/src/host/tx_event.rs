use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::AccountId;
use miden_objects::asset::{AssetVaultKey, FungibleAsset};
use miden_objects::note::{NoteInputs, NoteMetadata};
use miden_processor::AdviceMutation;

use crate::auth::SigningInputs;

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
    /// The data necessary to request a foreign account's data from the data store.
    ForeignAccount {
        /// The foreign account's ID.
        account_id: AccountId,
    },
    /// The data necessary to request an asset witness from the data store.
    AccountVaultAssetWitness {
        /// The account ID for whose vault a witness is requested.
        current_account_id: AccountId,
        /// The vault root identifying the asset vault from which a witness is requested.
        vault_root: Word,
        /// The asset for which a witness is requested.
        asset_key: AssetVaultKey,
    },
    /// The data necessary to request a storage map witness from the data store.
    AccountStorageMapWitness {
        /// The account ID for whose storage a witness is requested.
        current_account_id: AccountId,
        /// The root of the storage map in the account at the beginning of the transaction.
        map_root: Word,
        /// The raw map key for which a witness is requested.
        map_key: Word,
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
