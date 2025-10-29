use alloc::string::ToString;
use alloc::vec::Vec;
use std::collections::BTreeMap;

use thiserror::Error;

use crate::account::{AccountId, AccountType};
use crate::block::{
    AccountUpdateWitness,
    BlockNoteIndex,
    BlockNoteTree,
    BlockNumber,
    NullifierWitness,
    OutputNoteBatch,
    PartialAccountTree,
    PartialNullifierTree,
    ProposedBlock,
};
use crate::note::Nullifier;
use crate::transaction::PartialBlockchain;
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{AccountTreeError, FeeError, Felt, Hasher, NullifierTreeError, Word, ZERO};

/// The header of a block. It contains metadata about the block, commitments to the current
/// state of the chain and the hash of the proof that attests to the integrity of the chain.
///
/// A block header includes the following fields:
///
/// - `version` specifies the version of the protocol.
/// - `prev_block_commitment` is the hash of the previous block header.
/// - `block_num` is a unique sequential number of the current block.
/// - `chain_commitment` is a commitment to an MMR of the entire chain where each block is a leaf.
/// - `account_root` is a commitment to account database.
/// - `nullifier_root` is a commitment to the nullifier database.
/// - `note_root` is a commitment to all notes created in the current block.
/// - `tx_commitment` is a commitment to the set of transaction IDs which affected accounts in the
///   block.
/// - `tx_kernel_commitment` a commitment to all transaction kernels supported by this block.
/// - `proof_commitment` is the commitment of the block's STARK proof attesting to the correct state
///   transition.
/// - `fee_parameters` are the parameters defining the base fees and the native asset, see
///   [`FeeParameters`] for more details.
/// - `timestamp` is the time when the block was created, in seconds since UNIX epoch. Current
///   representation is sufficient to represent time up to year 2106.
/// - `sub_commitment` is a sequential hash of all fields except the note_root.
/// - `commitment` is a 2-to-1 hash of the sub_commitment and the note_root.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BlockHeader {
    version: u32,
    prev_block_commitment: Word,
    block_num: BlockNumber,
    chain_commitment: Word,
    account_root: Word,
    nullifier_root: Word,
    note_root: Word,
    tx_commitment: Word,
    signature: Word,
    fee_parameters: FeeParameters,
    timestamp: u32,
    sub_commitment: Word,
    commitment: Word,
}

impl BlockHeader {
    /// Creates a new block header.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u32,
        prev_block_commitment: Word,
        block_num: BlockNumber,
        chain_commitment: Word,
        account_root: Word,
        nullifier_root: Word,
        note_root: Word,
        tx_commitment: Word,
        signature: Word,
        fee_parameters: FeeParameters,
        timestamp: u32,
    ) -> Self {
        // compute block sub commitment
        let sub_commitment = Self::compute_sub_commitment(
            version,
            prev_block_commitment,
            chain_commitment,
            account_root,
            nullifier_root,
            tx_commitment,
            signature,
            &fee_parameters,
            timestamp,
            block_num,
        );

        // The sub commitment is merged with the note_root - hash(sub_commitment, note_root) to
        // produce the final hash. This is done to make the note_root easily accessible
        // without having to unhash the entire header. Having the note_root easily
        // accessible is useful when authenticating notes.
        let commitment = Hasher::merge(&[sub_commitment, note_root]);

        Self {
            version,
            prev_block_commitment,
            block_num,
            chain_commitment,
            account_root,
            nullifier_root,
            note_root,
            tx_commitment,
            signature,
            fee_parameters,
            timestamp,
            sub_commitment,
            commitment,
        }
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the protocol version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the commitment of the block header.
    pub fn commitment(&self) -> Word {
        self.commitment
    }

    /// Returns the sub commitment of the block header.
    ///
    /// The sub commitment is a sequential hash of all block header fields except the note root.
    /// This is used in the block commitment computation which is a 2-to-1 hash of the sub
    /// commitment and the note root [hash(sub_commitment, note_root)]. This procedure is used to
    /// make the note root easily accessible without having to unhash the entire header.
    pub fn sub_commitment(&self) -> Word {
        self.sub_commitment
    }

    /// Returns the commitment to the previous block header.
    pub fn prev_block_commitment(&self) -> Word {
        self.prev_block_commitment
    }

    /// Returns the block number.
    pub fn block_num(&self) -> BlockNumber {
        self.block_num
    }

    /// Returns the epoch to which this block belongs.
    ///
    /// This is the block number shifted right by [`BlockNumber::EPOCH_LENGTH_EXPONENT`].
    pub fn block_epoch(&self) -> u16 {
        self.block_num.block_epoch()
    }

    /// Returns the chain commitment.
    pub fn chain_commitment(&self) -> Word {
        self.chain_commitment
    }

    /// Returns the account database root.
    pub fn account_root(&self) -> Word {
        self.account_root
    }

    /// Returns the nullifier database root.
    pub fn nullifier_root(&self) -> Word {
        self.nullifier_root
    }

    /// Returns the note root.
    pub fn note_root(&self) -> Word {
        self.note_root
    }

    /// Returns the commitment to all transactions in this block.
    ///
    /// The commitment is computed as sequential hash of (`transaction_id`, `account_id`) tuples.
    /// This makes it possible for the verifier to link transaction IDs to the accounts which
    /// they were executed against.
    pub fn tx_commitment(&self) -> Word {
        self.tx_commitment
    }

    /// Returns the block signature.
    pub fn signature(&self) -> Word {
        self.signature
    }

    /// Returns a reference to the [`FeeParameters`] in this header.
    pub fn fee_parameters(&self) -> &FeeParameters {
        &self.fee_parameters
    }

    /// Returns the timestamp at which the block was created, in seconds since UNIX epoch.
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Returns the block number of the epoch block to which this block belongs.
    pub fn epoch_block_num(&self) -> BlockNumber {
        BlockNumber::from_epoch(self.block_epoch())
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Computes the sub commitment of the block header.
    ///
    /// The sub commitment is computed as a sequential hash of the following fields:
    /// `prev_block_commitment`, `chain_commitment`, `account_root`, `nullifier_root`, `note_root`,
    /// `tx_commitment`, `tx_kernel_commitment`, `signature`, `version`, `timestamp`,
    /// `block_num`, `native_asset_id`, `verification_base_fee` (all fields except the `note_root`).
    #[allow(clippy::too_many_arguments)]
    fn compute_sub_commitment(
        version: u32,
        prev_block_commitment: Word,
        chain_commitment: Word,
        account_root: Word,
        nullifier_root: Word,
        tx_commitment: Word,
        signature: Word,
        fee_parameters: &FeeParameters,
        timestamp: u32,
        block_num: BlockNumber,
    ) -> Word {
        let mut elements: Vec<Felt> = Vec::with_capacity(40);
        elements.extend_from_slice(prev_block_commitment.as_elements());
        elements.extend_from_slice(chain_commitment.as_elements());
        elements.extend_from_slice(account_root.as_elements());
        elements.extend_from_slice(nullifier_root.as_elements());
        elements.extend_from_slice(tx_commitment.as_elements());
        elements.extend_from_slice(signature.as_elements());
        elements.extend([block_num.into(), version.into(), timestamp.into(), ZERO]);
        elements.extend([
            fee_parameters.native_asset_id().suffix(),
            fee_parameters.native_asset_id().prefix().as_felt(),
            fee_parameters.verification_base_fee().into(),
            ZERO,
        ]);
        elements.extend([ZERO, ZERO, ZERO, ZERO]);
        Hasher::hash_elements(&elements)
    }
}

impl TryFrom<ProposedBlock> for BlockHeader {
    type Error = ProvenBlockError;

    fn try_from(proposed_block: ProposedBlock) -> Result<Self, Self::Error> {
        // Get the block number and timestamp of the new block and compute the tx commitment.
        // --------------------------------------------------------------------------------------------

        let block_num = proposed_block.block_num();
        let timestamp = proposed_block.timestamp();

        // Split the proposed block into its parts.
        // --------------------------------------------------------------------------------------------

        let (
            batches,
            account_updated_witnesses,
            output_note_batches,
            created_nullifiers,
            partial_blockchain,
            prev_block_header,
        ) = proposed_block.into_parts();

        let prev_block_commitment = prev_block_header.commitment();
        // For now we copy the parameters of the previous header, which means the parameters set on
        // the genesis block will be passed through. Eventually, the contained base fees will be
        // updated based on the demand in the currently proposed block.
        let fee_parameters = prev_block_header.fee_parameters().clone();

        // Compute the root of the block note tree.
        // --------------------------------------------------------------------------------------------

        let note_tree = compute_block_note_tree(&output_note_batches);
        let note_root = note_tree.root();

        // Insert the created nullifiers into the nullifier tree to compute its new root.
        // --------------------------------------------------------------------------------------------

        let (_created_nullifiers, new_nullifier_root) =
            compute_nullifiers(created_nullifiers, &prev_block_header, block_num)?;

        // Insert the state commitments of updated accounts into the account tree to compute its new
        // root.
        // --------------------------------------------------------------------------------------------

        let new_account_root =
            compute_account_root(&account_updated_witnesses, &prev_block_header)?;

        // Insert the previous block header into the block partial blockchain to get the new chain
        // commitment.
        // --------------------------------------------------------------------------------------------

        let new_chain_commitment = compute_chain_commitment(partial_blockchain, prev_block_header);

        // Aggregate the verified transactions of all batches.
        // --------------------------------------------------------------------------------------------

        let txs = batches.into_transactions();
        let tx_commitment = txs.commitment();

        // Construct the new block header.
        // --------------------------------------------------------------------------------------------

        // Currently undefined and reserved for future use.
        // See miden-base/1155.
        let version = 0;

        // For now, we're not actually proving the block.
        let proof_commitment = Word::empty();

        Ok(BlockHeader::new(
            version,
            prev_block_commitment,
            block_num,
            new_chain_commitment,
            new_account_root,
            new_nullifier_root,
            note_root,
            tx_commitment,
            proof_commitment,
            fee_parameters,
            timestamp,
        ))
    }
}

/// Computes the new account tree root after the given updates.
///
/// It uses a PartialMerkleTree for now while we use a SimpleSmt for the account tree. Once that is
/// updated to an Smt, we can use a PartialSmt instead.
fn compute_account_root(
    updated_accounts: &[(AccountId, AccountUpdateWitness)],
    prev_block_header: &BlockHeader,
) -> Result<Word, ProvenBlockError> {
    // If no accounts were updated, the account tree root is unchanged.
    if updated_accounts.is_empty() {
        return Ok(prev_block_header.account_root());
    }

    // First reconstruct the current account tree from the provided merkle paths.
    // If a witness points to a leaf where multiple account IDs share the same prefix, this will
    // return an error.
    let mut partial_account_tree = PartialAccountTree::with_witnesses(
        updated_accounts.iter().map(|(_, update_witness)| update_witness.to_witness()),
    )
    .map_err(|source| ProvenBlockError::AccountWitnessTracking { source })?;

    // Check the account tree root in the previous block header matches the reconstructed tree's
    // root.
    if prev_block_header.account_root() != partial_account_tree.root() {
        return Err(ProvenBlockError::StaleAccountTreeRoot {
            prev_block_account_root: prev_block_header.account_root(),
            stale_account_root: partial_account_tree.root(),
        });
    }

    // Second, update the account tree by inserting the new final account state commitments to
    // compute the new root of the account tree.
    // If an account ID's prefix already exists in the tree, this will return an error.
    // Note that we have inserted all witnesses that we want to update into the partial account
    // tree, so we should not run into the untracked key error.
    partial_account_tree
        .upsert_state_commitments(updated_accounts.iter().map(|(account_id, update_witness)| {
            (*account_id, update_witness.final_state_commitment())
        }))
        .map_err(|source| ProvenBlockError::AccountIdPrefixDuplicate { source })?;

    Ok(partial_account_tree.root())
}

/// Compute the block note tree from the output note batches.
fn compute_block_note_tree(output_note_batches: &[OutputNoteBatch]) -> BlockNoteTree {
    let output_notes_iter =
        output_note_batches.iter().enumerate().flat_map(|(batch_idx, notes)| {
            notes.iter().map(move |(note_idx_in_batch, note)| {
                (
                    // SAFETY: The proposed block contains at most the max allowed number of
                    // batches and each batch is guaranteed to contain at most
                    // the max allowed number of output notes.
                    BlockNoteIndex::new(batch_idx, *note_idx_in_batch)
                        .expect("max batches in block and max notes in batches should be enforced"),
                    note.id(),
                    *note.metadata(),
                )
            })
        });

    // SAFETY: We only construct proposed blocks that:
    // - do not contain duplicates
    // - contain at most the max allowed number of batches and each batch is guaranteed to contain
    //   at most the max allowed number of output notes.
    BlockNoteTree::with_entries(output_notes_iter)
        .expect("the output notes of the block should not contain duplicates and contain at most the allowed maximum")
}

/// Computes the new nullifier root by inserting the nullifier witnesses into a partial nullifier
/// tree and marking each nullifier as spent in the given block number. Returns the list of
/// nullifiers and the new nullifier tree root.
fn compute_nullifiers(
    created_nullifiers: BTreeMap<Nullifier, NullifierWitness>,
    prev_block_header: &BlockHeader,
    block_num: BlockNumber,
) -> Result<(Vec<Nullifier>, Word), ProvenBlockError> {
    // If no nullifiers were created, the nullifier tree root is unchanged.
    if created_nullifiers.is_empty() {
        return Ok((Vec::new(), prev_block_header.nullifier_root()));
    }

    let nullifiers: Vec<Nullifier> = created_nullifiers.keys().copied().collect();

    let mut partial_nullifier_tree = PartialNullifierTree::new();

    // First, reconstruct the current nullifier tree with the merkle paths of the nullifiers we want
    // to update.
    // Due to the guarantees of ProposedBlock we can safely assume that each nullifier is mapped to
    // its corresponding nullifier witness, so we don't have to check again whether they match.
    for witness in created_nullifiers.into_values() {
        partial_nullifier_tree
            .track_nullifier(witness)
            .map_err(ProvenBlockError::NullifierWitnessRootMismatch)?;
    }

    // Check the nullifier tree root in the previous block header matches the reconstructed tree's
    // root.
    if prev_block_header.nullifier_root() != partial_nullifier_tree.root() {
        return Err(ProvenBlockError::StaleNullifierTreeRoot {
            prev_block_nullifier_root: prev_block_header.nullifier_root(),
            stale_nullifier_root: partial_nullifier_tree.root(),
        });
    }

    // Second, mark each nullifier as spent in the tree. Note that checking whether each nullifier
    // is unspent is checked as part of the proposed block.

    // SAFETY: As mentioned above, we can safely assume that each nullifier's witness was
    // added and every nullifier should be tracked by the partial tree and
    // therefore updatable.
    partial_nullifier_tree.mark_spent(nullifiers.iter().copied(), block_num).expect(
      "nullifiers' merkle path should have been added to the partial tree and the nullifiers should be unspent",
    );

    Ok((nullifiers, partial_nullifier_tree.root()))
}

/// Adds the commitment of the previous block header to the partial blockchain to compute the new
/// chain commitment.
fn compute_chain_commitment(
    mut partial_blockchain: PartialBlockchain,
    prev_block_header: BlockHeader,
) -> Word {
    // SAFETY: This does not panic as long as the block header we're adding is the next one in the
    // chain which is validated as part of constructing a `ProposedBlock`.
    partial_blockchain.add_block(prev_block_header, true);
    partial_blockchain.peaks().hash_peaks()
}

// SERIALIZATION
// ================================================================================================

impl Serializable for BlockHeader {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.version.write_into(target);
        self.prev_block_commitment.write_into(target);
        self.block_num.write_into(target);
        self.chain_commitment.write_into(target);
        self.account_root.write_into(target);
        self.nullifier_root.write_into(target);
        self.note_root.write_into(target);
        self.tx_commitment.write_into(target);
        self.signature.write_into(target);
        self.fee_parameters.write_into(target);
        self.timestamp.write_into(target);
    }
}

impl Deserializable for BlockHeader {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let version = source.read()?;
        let prev_block_commitment = source.read()?;
        let block_num = source.read()?;
        let chain_commitment = source.read()?;
        let account_root = source.read()?;
        let nullifier_root = source.read()?;
        let note_root = source.read()?;
        let tx_commitment = source.read()?;
        let proof_commitment = source.read()?;
        let fee_parameters = source.read()?;
        let timestamp = source.read()?;

        Ok(Self::new(
            version,
            prev_block_commitment,
            block_num,
            chain_commitment,
            account_root,
            nullifier_root,
            note_root,
            tx_commitment,
            proof_commitment,
            fee_parameters,
            timestamp,
        ))
    }
}

// FEE PARAMETERS
// ================================================================================================

/// The fee-related parameters of a block.
///
/// This defines how to compute the fees of a transaction and which asset fees can be paid in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeParameters {
    /// The [`AccountId`] of the fungible faucet whose assets are accepted for fee payments in the
    /// transaction kernel, or in other words, the native asset of the blockchain.
    native_asset_id: AccountId,
    /// The base fee (in base units) capturing the cost for the verification of a transaction.
    verification_base_fee: u32,
}

impl FeeParameters {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`FeeParameters`] from the provided inputs.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided native asset ID is not a fungible faucet account ID.
    pub fn new(native_asset_id: AccountId, verification_base_fee: u32) -> Result<Self, FeeError> {
        if !matches!(native_asset_id.account_type(), AccountType::FungibleFaucet) {
            return Err(FeeError::NativeAssetIdNotFungible {
                account_type: native_asset_id.account_type(),
            });
        }

        Ok(Self { native_asset_id, verification_base_fee })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`AccountId`] of the faucet whose assets are accepted for fee payments in the
    /// transaction kernel, or in other words, the native asset of the blockchain.
    pub fn native_asset_id(&self) -> AccountId {
        self.native_asset_id
    }

    /// Returns the base fee capturing the cost for the verification of a transaction.
    pub fn verification_base_fee(&self) -> u32 {
        self.verification_base_fee
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for FeeParameters {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.native_asset_id.write_into(target);
        self.verification_base_fee.write_into(target);
    }
}

impl Deserializable for FeeParameters {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let native_asset_id = source.read()?;
        let verification_base_fee = source.read()?;

        Self::new(native_asset_id, verification_base_fee)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TODO: rm / rename this
#[derive(Debug, Error)]
pub enum ProvenBlockError {
    #[error("nullifier witness has a different root than the current nullifier tree root")]
    NullifierWitnessRootMismatch(#[source] NullifierTreeError),

    #[error("failed to track account witness")]
    AccountWitnessTracking { source: AccountTreeError },

    #[error("account ID prefix already exists in the tree")]
    AccountIdPrefixDuplicate { source: AccountTreeError },

    #[error(
        "account tree root of the previous block header is {prev_block_account_root} but the root of the partial tree computed from account witnesses is {stale_account_root}, indicating that the witnesses are stale"
    )]
    StaleAccountTreeRoot {
        prev_block_account_root: Word,
        stale_account_root: Word,
    },

    #[error(
        "nullifier tree root of the previous block header is {prev_block_nullifier_root} but the root of the partial tree computed from nullifier witnesses is {stale_nullifier_root}, indicating that the witnesses are stale"
    )]
    StaleNullifierTreeRoot {
        prev_block_nullifier_root: Word,
        stale_nullifier_root: Word,
    },
}
// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use miden_core::Word;
    use winter_rand_utils::rand_value;

    use super::*;
    use crate::testing::account_id::ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET;

    #[test]
    fn test_serde() {
        let chain_commitment = rand_value::<Word>();
        let note_root = rand_value::<Word>();
        let header = BlockHeader::mock(0, Some(chain_commitment), Some(note_root), &[]);
        let serialized = header.to_bytes();
        let deserialized = BlockHeader::read_from_bytes(&serialized).unwrap();

        assert_eq!(deserialized, header);
    }

    /// Tests that the fee parameters constructor fails when the provided account ID is not a
    /// fungible faucet.
    #[test]
    fn fee_parameters_fail_when_native_asset_is_not_fungible() {
        assert_matches!(
            FeeParameters::new(ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET.try_into().unwrap(), 0)
                .unwrap_err(),
            FeeError::NativeAssetIdNotFungible { .. }
        );
    }
}
