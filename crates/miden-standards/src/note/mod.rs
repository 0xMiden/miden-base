use alloc::vec::Vec;

use miden_protocol::account::AccountId;
use miden_protocol::asset::Asset;
use miden_protocol::block::BlockNumber;
use miden_protocol::crypto::rand::FeltRng;
use miden_protocol::note::{
    Note,
    NoteAssets,
    NoteDetails,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteTag,
    NoteType,
};
use miden_protocol::{Felt, NoteError, Word};
use utils::build_swap_tag;

pub mod mint_inputs;
pub mod utils;

mod network_account_target;
pub use network_account_target::NetworkAccountTarget;

mod well_known_note_attachment;
pub use well_known_note_attachment::WellKnownNoteAttachment;

mod well_known_note;
pub use mint_inputs::MintNoteInputs;
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
    // TODO(note_attachment): Replace with note attachment.
    _aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let serial_num = rng.draw_word();
    let recipient = utils::build_p2id_recipient(target, serial_num)?;

    let tag = NoteTag::with_account_target(target);

    let metadata = NoteMetadata::new(sender, note_type, tag);
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
    // TODO(note_attachment): Replace with note attachment.
    _aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let serial_num = rng.draw_word();
    let recipient =
        utils::build_p2ide_recipient(target, reclaim_height, timelock_height, serial_num)?;
    let tag = NoteTag::with_account_target(target);

    let metadata = NoteMetadata::new(sender, note_type, tag);
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
    // TODO(note_attachment): Replace with note attachment.
    _swap_note_aux: Felt,
    payback_note_type: NoteType,
    // TODO(note_attachment): Replace with note attachment.
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
    let payback_tag = NoteTag::with_account_target(sender);

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
    let tag = build_swap_tag(swap_note_type, &offered_asset, &requested_asset);
    let serial_num = rng.draw_word();

    // build the outgoing note
    let metadata = NoteMetadata::new(sender, swap_note_type, tag);
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
/// will mint the specified amount of fungible assets and create either a PRIVATE or PUBLIC
/// output note depending on the input configuration. The MINT note uses note-based authentication,
/// checking if the note sender equals the faucet owner to authorize minting.
///
/// MINT notes are always PUBLIC (for network execution). Output notes can be either PRIVATE
/// or PUBLIC depending on the MintNoteInputs variant used.
///
/// The passed-in `rng` is used to generate a serial number for the note. The note's tag
/// is automatically set to the faucet's account ID for proper routing.
///
/// # Parameters
/// - `faucet_id`: The account ID of the network faucet that will mint the assets
/// - `sender`: The account ID of the note creator (must be the faucet owner)
/// - `mint_inputs`: The input configuration specifying private or public output mode
/// - `aux`: Auxiliary data for the MINT note
/// - `rng`: Random number generator for creating the serial number
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_mint_note<R: FeltRng>(
    faucet_id: AccountId,
    sender: AccountId,
    mint_inputs: MintNoteInputs,
    // TODO(note_attachment): Replace with note attachment.
    _aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let note_script = WellKnownNote::MINT.script();
    let serial_num = rng.draw_word();

    // MINT notes are always public for network execution
    let note_type = NoteType::Public;

    // Convert MintNoteInputs to NoteInputs
    let inputs = NoteInputs::from(mint_inputs);

    let tag = NoteTag::with_account_target(faucet_id);

    let metadata = NoteMetadata::new(sender, note_type, tag);
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
    // TODO(note_attachment): Replace with note attachment.
    _aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let note_script = WellKnownNote::BURN.script();
    let serial_num = rng.draw_word();

    // BURN notes are always public
    let note_type = NoteType::Public;

    let inputs = NoteInputs::new(vec![])?;
    let tag = NoteTag::with_account_target(faucet_id);

    let metadata = NoteMetadata::new(sender, note_type, tag);
    let assets = NoteAssets::new(vec![fungible_asset])?; // BURN notes contain the asset to burn
    let recipient = NoteRecipient::new(serial_num, note_script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}
