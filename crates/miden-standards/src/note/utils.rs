use miden_protocol::account::AccountId;
use miden_protocol::asset::Asset;
use miden_protocol::block::BlockNumber;
use miden_protocol::note::{NoteInputs, NoteRecipient, NoteTag, NoteType};
use miden_protocol::{Felt, NoteError, Word};

use super::well_known_note::WellKnownNote;

/// Creates a [NoteRecipient] for the P2ID note.
///
/// Notes created with this recipient will be P2ID notes consumable by the specified target
/// account.
pub fn build_p2id_recipient(
    target: AccountId,
    serial_num: Word,
) -> Result<NoteRecipient, NoteError> {
    let note_script = WellKnownNote::P2ID.script();
    let note_inputs = NoteInputs::new(vec![target.suffix(), target.prefix().as_felt()])?;

    Ok(NoteRecipient::new(serial_num, note_script, note_inputs))
}

/// Creates a [NoteRecipient] for the P2IDE note.
///
/// Notes created with this recipient will be P2IDE notes consumable by the specified target
/// account.
pub fn build_p2ide_recipient(
    target: AccountId,
    reclaim_block_height: Option<BlockNumber>,
    timelock_block_height: Option<BlockNumber>,
    serial_num: Word,
) -> Result<NoteRecipient, NoteError> {
    let note_script = WellKnownNote::P2IDE.script();

    let reclaim_height_u32 = reclaim_block_height.map_or(0, |bn| bn.as_u32());
    let timelock_height_u32 = timelock_block_height.map_or(0, |bn| bn.as_u32());

    let note_inputs = NoteInputs::new(vec![
        target.suffix(),
        target.prefix().into(),
        Felt::new(reclaim_height_u32 as u64),
        Felt::new(timelock_height_u32 as u64),
    ])?;

    Ok(NoteRecipient::new(serial_num, note_script, note_inputs))
}

/// Returns a note tag for a swap note with the specified parameters.
///
/// The tag is laid out as follows:
///
/// ```text
/// [
///   note_type (2 bits) | script_root (14 bits)
///   | offered_asset_faucet_id (8 bits) | requested_asset_faucet_id (8 bits)
/// ]
/// ```
///
/// The script root serves as the use case identifier of the SWAP tag.
pub fn build_swap_tag(
    note_type: NoteType,
    offered_asset: &Asset,
    requested_asset: &Asset,
) -> NoteTag {
    let swap_root_bytes = WellKnownNote::SWAP.script().root().as_bytes();
    // Construct the swap use case ID from the 14 most significant bits of the script root. This
    // leaves the two most significant bits zero.
    let mut swap_use_case_id = (swap_root_bytes[0] as u16) << 6;
    swap_use_case_id |= (swap_root_bytes[1] >> 2) as u16;

    // Get bits 0..8 from the faucet IDs of both assets which will form the tag payload.
    let offered_asset_id: u64 = offered_asset.faucet_id_prefix().into();
    let offered_asset_tag = (offered_asset_id >> 56) as u8;

    let requested_asset_id: u64 = requested_asset.faucet_id_prefix().into();
    let requested_asset_tag = (requested_asset_id >> 56) as u8;

    let asset_pair = ((offered_asset_tag as u16) << 8) | (requested_asset_tag as u16);

    let tag =
        ((note_type as u8 as u32) << 30) | ((swap_use_case_id as u32) << 16) | asset_pair as u32;

    NoteTag::new(tag)
}

#[cfg(test)]
mod tests {
    use miden_protocol::account::{AccountIdVersion, AccountStorageMode, AccountType};
    use miden_protocol::asset::{FungibleAsset, NonFungibleAsset, NonFungibleAssetDetails};
    use miden_protocol::{self};

    use super::*;

    #[test]
    fn swap_tag() {
        // Construct an ID that starts with 0xcdb1.
        let mut fungible_faucet_id_bytes = [0; 15];
        fungible_faucet_id_bytes[0] = 0xcd;
        fungible_faucet_id_bytes[1] = 0xb1;

        // Construct an ID that starts with 0xabec.
        let mut non_fungible_faucet_id_bytes = [0; 15];
        non_fungible_faucet_id_bytes[0] = 0xab;
        non_fungible_faucet_id_bytes[1] = 0xec;

        let offered_asset = Asset::Fungible(
            FungibleAsset::new(
                AccountId::dummy(
                    fungible_faucet_id_bytes,
                    AccountIdVersion::Version0,
                    AccountType::FungibleFaucet,
                    AccountStorageMode::Public,
                ),
                2500,
            )
            .unwrap(),
        );

        let requested_asset = Asset::NonFungible(
            NonFungibleAsset::new(
                &NonFungibleAssetDetails::new(
                    AccountId::dummy(
                        non_fungible_faucet_id_bytes,
                        AccountIdVersion::Version0,
                        AccountType::NonFungibleFaucet,
                        AccountStorageMode::Public,
                    )
                    .prefix(),
                    vec![0xaa, 0xbb, 0xcc, 0xdd],
                )
                .unwrap(),
            )
            .unwrap(),
        );

        // The fungible ID starts with 0xcdb1.
        // The non fungible ID starts with 0xabec.
        // The expected tag payload is thus 0xcdab.
        let expected_asset_pair = 0xcdab;

        let note_type = NoteType::Public;
        let actual_tag = build_swap_tag(note_type, &offered_asset, &requested_asset);

        assert_eq!(actual_tag.as_u32() as u16, expected_asset_pair, "asset pair should match");
        assert_eq!((actual_tag.as_u32() >> 30) as u8, note_type as u8, "note type should match");
        // Check the 8 bits of the first script root byte.
        assert_eq!(
            (actual_tag.as_u32() >> 22) as u8,
            WellKnownNote::SWAP.script().root().as_bytes()[0],
            "swap script root byte 0 should match"
        );
        // Extract the 6 bits of the second script root byte and shift for comparison.
        assert_eq!(
            ((actual_tag.as_u32() & 0b00000000_00111111_00000000_00000000) >> 16) as u8,
            WellKnownNote::SWAP.script().root().as_bytes()[1] >> 2,
            "swap script root byte 1 should match with the lower two bits set to zero"
        );
    }
}
