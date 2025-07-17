use alloc::vec::Vec;

use miden_objects::{
    Felt, NoteError, Word,
    account::AccountId,
    asset::Asset,
    block::BlockNumber,
    crypto::rand::FeltRng,
    note::{
        Note, NoteAssets, NoteDetails, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient,
        NoteTag, NoteType,
    },
};
use utils::build_swap_tag;
use well_known_note::WellKnownNote;

pub mod utils;
pub mod well_known_note;

// STANDARDIZED SCRIPTS
// ================================================================================================

/// Generates a P2ID note - Pay-to-ID note.
///
/// This script enables the transfer of assets from the `sender` account to the `target` account
/// by specifying the target's account ID.
///
/// The passed-in `rng` is used to generate a serial number for the note. The returned note's tag
/// is set to the target's account ID.
///
/// # Errors
/// Returns an error if deserialization or compilation of the `P2ID` script fails.
pub fn create_p2id_note<R: FeltRng>(
    sender: AccountId,
    target: AccountId,
    assets: Vec<Asset>,
    note_type: NoteType,
    aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let serial_num = rng.draw_word();
    let recipient = utils::build_p2id_recipient(target, serial_num)?;

    let tag = NoteTag::from_account_id(target);

    let metadata = NoteMetadata::new(sender, note_type, tag, NoteExecutionHint::always(), aux)?;
    let vault = NoteAssets::new(assets)?;

    Ok(Note::new(vault, metadata, recipient))
}

/// Generates a P2IDE note - Pay-to-ID note with optional reclaim after a certain block height and
/// optional timelock.
///
/// This script enables the transfer of assets from the `sender` account to the `target`
/// account by specifying the target's account ID. It adds the optional possibility for the
/// sender to reclaiming the assets if the note has not been consumed by the target within the
/// specified timeframe and the optional possibility to add a timelock to the asset transfer.
///
/// The passed-in `rng` is used to generate a serial number for the note. The returned note's tag
/// is set to the target's account ID.
///
/// # Errors
/// Returns an error if deserialization or compilation of the `P2ID` script fails.
pub fn create_p2ide_note<R: FeltRng>(
    sender: AccountId,
    target: AccountId,
    assets: Vec<Asset>,
    reclaim_height: Option<BlockNumber>,
    timelock_height: Option<BlockNumber>,
    note_type: NoteType,
    aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let serial_num = rng.draw_word();
    let recipient =
        utils::build_p2ide_recipient(target, reclaim_height, timelock_height, serial_num)?;
    let tag = NoteTag::from_account_id(target);

    let execution_hint = match timelock_height {
        Some(height) => NoteExecutionHint::after_block(height)?,
        None => NoteExecutionHint::always(),
    };

    let metadata = NoteMetadata::new(sender, note_type, tag, execution_hint, aux)?;
    let vault = NoteAssets::new(assets)?;

    Ok(Note::new(vault, metadata, recipient))
}

/// Generates a SWAP note - swap of assets between two accounts - and returns the note as well as
/// [NoteDetails] for the payback note.
///
/// This script enables a swap of 2 assets between the `sender` account and any other account that
/// is willing to consume the note. The consumer will receive the `offered_asset` and will create a
/// new P2ID note with `sender` as target, containing the `requested_asset`.
///
/// # Errors
/// Returns an error if deserialization or compilation of the `SWAP` script fails.
pub fn create_swap_note<R: FeltRng>(
    sender: AccountId,
    offered_asset: Asset,
    requested_asset: Asset,
    swap_note_type: NoteType,
    swap_note_aux: Felt,
    payback_note_type: NoteType,
    payback_note_aux: Felt,
    rng: &mut R,
) -> Result<(Note, NoteDetails), NoteError> {
    let note_script = WellKnownNote::SWAP.script();

    let payback_serial_num = rng.draw_word();
    let payback_recipient = utils::build_p2id_recipient(sender, payback_serial_num)?;

    let payback_recipient_word: Word = payback_recipient.digest();
    let requested_asset_word: Word = requested_asset.into();
    let payback_tag = NoteTag::from_account_id(sender);

    let inputs = NoteInputs::new(vec![
        requested_asset_word[0],
        requested_asset_word[1],
        requested_asset_word[2],
        requested_asset_word[3],
        payback_recipient_word[0],
        payback_recipient_word[1],
        payback_recipient_word[2],
        payback_recipient_word[3],
        NoteExecutionHint::always().into(),
        payback_note_type.into(),
        payback_note_aux,
        payback_tag.into(),
    ])?;

    // build the tag for the SWAP use case
    let tag = build_swap_tag(swap_note_type, &offered_asset, &requested_asset)?;
    let serial_num = rng.draw_word();

    // build the outgoing note
    let metadata =
        NoteMetadata::new(sender, swap_note_type, tag, NoteExecutionHint::always(), swap_note_aux)?;
    let assets = NoteAssets::new(vec![offered_asset])?;
    let recipient = NoteRecipient::new(serial_num, note_script, inputs);
    let note = Note::new(assets, metadata, recipient);

    // build the payback note details
    let payback_assets = NoteAssets::new(vec![requested_asset])?;
    let payback_note = NoteDetails::new(payback_assets, payback_recipient);

    Ok((note, payback_note))
}
