use alloc::vec::Vec;

use miden_objects::account::AccountId;
use miden_objects::asset::Asset;
use miden_objects::block::BlockNumber;
use miden_objects::crypto::rand::FeltRng;
use miden_objects::note::{
    Note,
    NoteAssets,
    NoteDetails,
    NoteExecutionHint,
    NoteMetadata,
    NoteRecipient,
    NoteStorage,
    NoteTag,
    NoteType,
};
use miden_objects::{Felt, NoteError, Word};
use utils::build_swap_tag;

pub mod utils;

mod well_known_note;
pub use well_known_note::{NoteConsumptionStatus, WellKnownNote};

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
    if requested_asset == offered_asset {
        return Err(NoteError::other("requested asset same as offered asset"));
    }

    let note_script = WellKnownNote::SWAP.script();

    let payback_serial_num = rng.draw_word();
    let payback_recipient = utils::build_p2id_recipient(sender, payback_serial_num)?;

    let payback_recipient_word: Word = payback_recipient.digest();
    let requested_asset_word: Word = requested_asset.into();
    let payback_tag = NoteTag::from_account_id(sender);

    let inputs = NoteStorage::new(vec![
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

/// Generates a MINT note - a note that instructs a network faucet to mint fungible assets.
///
/// This script enables the creation of a PUBLIC note that, when consumed by a network faucet,
/// will mint the specified amount of fungible assets and create a PRIVATE note with the given
/// RECIPIENT. The MINT note uses note-based authentication, checking if the note sender equals
/// the faucet owner to authorize minting.
///
/// MINT notes are always PUBLIC (for network execution) and output notes are always PRIVATE
/// (TODO: enable public output note creation from MINT note consumption).
///
/// The passed-in `rng` is used to generate a serial number for the note. The note's tag
/// is automatically set to the faucet's account ID for proper routing.
///
/// # Parameters
/// - `faucet_id`: The account ID of the network faucet that will mint the assets
/// - `sender`: The account ID of the note creator (must be the faucet owner)
/// - `target_recipient`: The recipient digest for the output P2ID note that will receive the minted
///   assets
/// - `output_note_tag`: The tag for the output P2ID note
/// - `amount`: The amount of fungible assets to mint
/// - `aux`: Auxiliary data for the MINT note
/// - `output_note_aux`: Auxiliary data for the output P2ID note
/// - `rng`: Random number generator for creating the serial number
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_mint_note<R: FeltRng>(
    faucet_id: AccountId,
    sender: AccountId,
    target_recipient: Word,
    output_note_tag: Felt,
    amount: Felt,
    aux: Felt,
    output_note_aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let note_script = WellKnownNote::MINT.script();
    let serial_num = rng.draw_word();

    // MINT notes are always public for network execution
    let note_type = NoteType::Public;
    // Output notes are always private (for now)
    let output_note_type = NoteType::Private;

    let execution_hint = NoteExecutionHint::always();

    let inputs = NoteStorage::new(vec![
        target_recipient[0],
        target_recipient[1],
        target_recipient[2],
        target_recipient[3],
        execution_hint.into(),
        output_note_type.into(),
        output_note_aux,
        output_note_tag,
        amount,
    ])?;

    let tag = NoteTag::from_account_id(faucet_id);

    let metadata = NoteMetadata::new(sender, note_type, tag, execution_hint, aux)?;
    let assets = NoteAssets::new(vec![])?; // MINT notes have no assets
    let recipient = NoteRecipient::new(serial_num, note_script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}

/// Generates a BURN note - a note that instructs a faucet to burn a fungible asset.
///
/// This script enables the creation of a PUBLIC note that, when consumed by a faucet (either basic
/// or network), will burn the fungible assets contained in the note. Both basic and network
/// fungible faucets export the same `burn` procedure with identical MAST roots, allowing
/// a single BURN note script to work with either faucet type.
///
/// BURN notes are always PUBLIC for network execution.
///
/// The passed-in `rng` is used to generate a serial number for the note. The note's tag
/// is automatically set to the faucet's account ID for proper routing.
///
/// # Parameters
/// - `sender`: The account ID of the note creator
/// - `faucet_id`: The account ID of the faucet that will burn the assets
/// - `fungible_asset`: The fungible asset to be burned
/// - `aux`: Auxiliary data for the note
/// - `rng`: Random number generator for creating the serial number
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_burn_note<R: FeltRng>(
    sender: AccountId,
    faucet_id: AccountId,
    fungible_asset: Asset,
    aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let note_script = WellKnownNote::BURN.script();
    let serial_num = rng.draw_word();

    // BURN notes are always public
    let note_type = NoteType::Public;
    // Use always execution hint for BURN notes
    let execution_hint = NoteExecutionHint::always();

    let inputs = NoteStorage::new(vec![])?;
    let tag = NoteTag::from_account_id(faucet_id);

    let metadata = NoteMetadata::new(sender, note_type, tag, execution_hint, aux)?;
    let assets = NoteAssets::new(vec![fungible_asset])?; // BURN notes contain the asset to burn
    let recipient = NoteRecipient::new(serial_num, note_script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}
