use alloc::vec::Vec;

use miden_protocol::account::{AccountCodeInterface, AccountId};
use miden_protocol::asset::AssetId;
use miden_protocol::note::{NoteAssets, NoteRecipient, NoteTag, NoteType};
use miden_protocol::transaction::{TransactionScript, TransactionScriptRoot};
use miden_protocol::utils::sync::LazyLock;
use miden_protocol::vm::AdviceMap;
use miden_protocol::{Felt, Hasher, WORD_SIZE, Word};
use thiserror::Error;

use crate::account::pass_through::PassThrough;
use crate::account::wallets::BasicWallet;
use crate::note::P2idNoteStorage;
use crate::tx_script::transaction_script;

// CONSTANTS
// ================================================================================================

/// Path to the `single_p2id` pass-through transaction script procedure in the standards library,
/// assembled from `asm/standards/tx_scripts/pass_through/single_p2id.masm`.
const PASS_THROUGH_SINGLE_P2ID_TX_SCRIPT_PATH: &str =
    "::miden::standards::tx_scripts::pass_through::single_p2id::main";

/// The `@locals` frame of the script's `forward_assets`, which the payload is piped into.
const MASM_NUM_LOCALS: usize = 75;

/// The loop-state locals that follow the payload in that frame.
const MASM_NUM_LOOP_STATE_LOCALS: usize = 3;

// A tripwire, not a proof: both constants above are hand-copied from the script, so this catches a
// change to `MAX_ASSETS_PER_NOTE` on the Rust side. The MASM side fails to assemble instead, since
// the assembler rejects a static local index past the frame.
const _: () = assert!(
    PassThroughSingleP2idTransactionScript::PAYLOAD_HEADER_NUM_ELEMENTS
        + PassThroughSingleP2idTransactionScript::MAX_ASSET_IDS * WORD_SIZE
        + MASM_NUM_LOOP_STATE_LOCALS
        <= MASM_NUM_LOCALS,
    "the payload the script accepts must fit the @locals frame in \
     asm/standards/tx_scripts/pass_through/single_p2id.masm"
);

// PASS-THROUGH SINGLE P2ID TRANSACTION SCRIPT
// ================================================================================================

static PASS_THROUGH_SINGLE_P2ID_TX_SCRIPT: LazyLock<TransactionScript> =
    LazyLock::new(|| transaction_script(PASS_THROUGH_SINGLE_P2ID_TX_SCRIPT_PATH));

/// The canonical transaction script that forwards the account's balance of the listed assets into
/// a single P2ID output note.
///
/// The state of the account it executes against does not change: it assumes the input notes
/// already deposited their assets into the account's vault, and moves the whole balance of each
/// listed asset into one P2ID note addressed to `target`, so the account's vault delta is zero.
/// Its commitment is unchanged as long as the auth procedure neither bumps the nonce nor funds a
/// fee note from the vault.
///
/// Listing assets rather than notes is what makes the script's cost depend on how many assets it
/// lists, not on how many notes the transaction consumes. The account must not hold any of the
/// listed assets of its own, or it moves more out of the vault than was deposited; and the payload
/// must list every asset the input notes deposit, or what is left behind stays in the vault. Both
/// change the account's commitment. No auth component that rejects a changed account ships yet -
/// `AuthPassThrough` arrives in [#3733](https://github.com/0xMiden/protocol/pull/3733) - so with
/// an auth that leaves the commitment alone when nothing changed, such as [`NoAuth`] or
/// [`AuthNetworkAccount`], both mistakes are silent: the nonce is bumped and the transaction
/// succeeds.
///
/// A successful transaction does not imply the listed assets reached `target`. A note script the
/// transaction consumes can sweep them first (see [`PassThrough`]), after which this script's own
/// sweep is a no-op and the vault ends as it started either way.
///
/// The payload is embedded into the script's MAST forest and committed to by `TX_SCRIPT_ARGS`, so
/// a single [`PassThroughSingleP2idTransactionScript::script_root`] covers every target, serial
/// number and asset set, and callers only have to set the script and its arguments:
///
/// ```ignore
/// let script =
///     PassThroughSingleP2idTransactionScript::new(&interface, target, note_type, serial, ids)?;
/// let tx_args = TransactionArgs::new(AdviceMap::default())
///     .with_tx_script_and_args(script.tx_script().clone(), script.tx_script_args());
/// ```
///
/// [`AuthNetworkAccount`]: crate::account::auth::AuthNetworkAccount
/// [`NoAuth`]: crate::account::auth::NoAuth
/// [`PassThrough`]: crate::account::pass_through::PassThrough
/// [`BasicWallet`]: crate::account::wallets::BasicWallet
#[derive(Debug, Clone)]
pub struct PassThroughSingleP2idTransactionScript {
    script: TransactionScript,
    tx_script_args: Word,
    output_note_recipient: NoteRecipient,
    output_note_tag: NoteTag,
    output_note_type: NoteType,
}

impl PassThroughSingleP2idTransactionScript {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Number of elements in the payload header: `[target_id_suffix, target_id_prefix, tag,
    /// note_type]` followed by `SERIAL_NUM`. One asset ID word follows per listed asset.
    pub const PAYLOAD_HEADER_NUM_ELEMENTS: usize = 8;

    /// Element offset of the output note's serial number within the payload header.
    const SERIAL_NUM_OFFSET: usize = 4;

    /// Maximum number of asset IDs the payload may list: listing more assets than fit into a
    /// single note could never be forwarded into one.
    pub const MAX_ASSET_IDS: usize = NoteAssets::MAX_NUM_ASSETS;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Builds a pass-through script forwarding the balance of every asset in `asset_ids` out of the
    /// account described by `interface`, into a P2ID note of type `note_type` addressed to
    /// `target`, carrying `serial_number`.
    ///
    /// `asset_ids` must list every asset the transaction's input notes deposit; an unlisted asset
    /// stays in the vault and changes the account.
    ///
    /// `serial_number` must be unique per transaction, as for any note: two notes sharing a target,
    /// an asset set and a serial number have the same ID and nullifier. Note that the pass-through
    /// account's state is constant, so the `(account, nonce)` tuple other standard notes derive a
    /// serial number from is not available here.
    ///
    /// The note's tag is derived as [`NoteTag::with_account_target`], matching the tag a
    /// Rust-built [`P2idNote`](crate::note::P2idNote) carries.
    ///
    /// # Errors
    ///
    /// Returns an error if more than [`Self::MAX_ASSET_IDS`] asset IDs are given, or if the
    /// account does not expose the procedures the script and its input notes call.
    pub fn new(
        interface: &AccountCodeInterface,
        target: AccountId,
        note_type: NoteType,
        serial_number: Word,
        asset_ids: impl IntoIterator<Item = AssetId>,
    ) -> Result<Self, PassThroughTransactionScriptError> {
        // `create_note` and `sweep_asset_to_note` are what the script itself calls; `receive_asset`
        // is what the input notes deposit through, without which there is nothing to forward.
        let supports_pass_through = interface.contains([
            PassThrough::sweep_asset_to_note_root(),
            BasicWallet::create_note_root(),
            BasicWallet::receive_asset_root(),
        ]);
        if !supports_pass_through {
            return Err(PassThroughTransactionScriptError::UnsupportedAccountInterface);
        }

        let asset_ids: Vec<AssetId> = asset_ids.into_iter().collect();
        if asset_ids.len() > Self::MAX_ASSET_IDS {
            return Err(PassThroughTransactionScriptError::TooManyAssetIds {
                actual: asset_ids.len(),
                max: Self::MAX_ASSET_IDS,
            });
        }

        let output_note_tag = NoteTag::with_account_target(target);
        let output_note_recipient = P2idNoteStorage::new(target).into_recipient(serial_number);

        let payload = encode_payload(target, output_note_tag, note_type, serial_number, &asset_ids);
        let tx_script_args = Hasher::hash_elements(&payload);

        // Embed the payload the script reads from the advice provider into the script's MAST
        // forest, so it is loaded automatically and callers only have to set the script and its
        // arguments.
        let mut advice_map = AdviceMap::default();
        advice_map.insert(tx_script_args, payload);

        Ok(Self {
            script: PASS_THROUGH_SINGLE_P2ID_TX_SCRIPT.clone().with_advice_map(advice_map),
            tx_script_args,
            output_note_recipient,
            output_note_tag,
            output_note_type: note_type,
        })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// The transaction script, with the payload embedded in its MAST forest's advice map.
    pub fn tx_script(&self) -> &TransactionScript {
        &self.script
    }

    /// The `TX_SCRIPT_ARGS` word the script reads its payload under: the payload's commitment.
    pub fn tx_script_args(&self) -> Word {
        self.tx_script_args
    }

    /// The recipient of the P2ID note the script creates, for callers that have to register it as
    /// an expected output recipient.
    pub fn output_note_recipient(&self) -> &NoteRecipient {
        &self.output_note_recipient
    }

    /// The tag of the P2ID note the script creates.
    pub fn output_note_tag(&self) -> NoteTag {
        self.output_note_tag
    }

    /// The type of the P2ID note the script creates.
    pub fn output_note_type(&self) -> NoteType {
        self.output_note_type
    }

    /// The [`TransactionScriptRoot`] of the canonical script, which is independent of the payload.
    pub fn script_root() -> TransactionScriptRoot {
        PASS_THROUGH_SINGLE_P2ID_TX_SCRIPT.root()
    }
}

impl From<PassThroughSingleP2idTransactionScript> for TransactionScript {
    fn from(script: PassThroughSingleP2idTransactionScript) -> Self {
        script.script
    }
}

// PASS-THROUGH TRANSACTION SCRIPT ERROR
// ================================================================================================

/// Errors that can occur while building a [`PassThroughSingleP2idTransactionScript`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PassThroughTransactionScriptError {
    #[error("pass-through payload lists {actual} assets but at most {max} fit into one note")]
    TooManyAssetIds { actual: usize, max: usize },
    #[error(
        "account does not expose the `sweep_asset_to_note`, `create_note` and `receive_asset` \
         procedures which are needed to support the pass-through script generation"
    )]
    UnsupportedAccountInterface,
}

// PAYLOAD ENCODING
// ================================================================================================

/// Encodes the script's parameters into the payload it loads from the advice map.
///
/// ```text
/// HEADER_WORD_0: [target_id_suffix, target_id_prefix, tag, note_type]
/// HEADER_WORD_1: SERIAL_NUM
/// WORD_2+:       one ASSET_ID per asset to forward
/// ```
fn encode_payload(
    target: AccountId,
    tag: NoteTag,
    note_type: NoteType,
    serial_number: Word,
    asset_ids: &[AssetId],
) -> Vec<Felt> {
    let mut payload = alloc::vec![
        target.suffix(),
        target.prefix().as_felt(),
        Felt::from(tag),
        Felt::from(note_type),
    ];
    debug_assert_eq!(
        payload.len(),
        PassThroughSingleP2idTransactionScript::SERIAL_NUM_OFFSET,
        "the serial number should start at the advertised offset"
    );

    payload.extend(serial_number.iter());
    debug_assert_eq!(
        payload.len(),
        PassThroughSingleP2idTransactionScript::PAYLOAD_HEADER_NUM_ELEMENTS,
        "the header size should match the advertised constant"
    );

    for asset_id in asset_ids {
        payload.extend(asset_id.to_word().iter());
    }

    payload
}
