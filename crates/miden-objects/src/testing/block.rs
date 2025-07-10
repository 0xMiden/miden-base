#[cfg(not(target_family = "wasm"))]
use winter_rand_utils::{rand_array, rand_value};

use crate::{
    Word,
    account::Account,
    block::{AccountTree, BlockHeader, BlockNumber},
};

impl BlockHeader {
    /// Creates a mock block. The account tree is formed from the provided `accounts`,
    /// and the chain commitment and note root are set to the provided `chain_commitment` and
    /// `note_root` values respectively.
    ///
    /// For non-WASM targets, the remaining header values are initialized randomly. For WASM
    /// targets, values are initialized to [Default::default()]
    pub fn mock(
        block_num: impl Into<BlockNumber>,
        chain_commitment: Option<Word>,
        note_root: Option<Word>,
        accounts: &[Account],
        tx_kernel_commitment: Word,
    ) -> Self {
        let acct_db =
            AccountTree::with_entries(accounts.iter().map(|acct| (acct.id(), acct.commitment())))
                .expect("failed to create account db");
        let account_root = acct_db.root();

        #[cfg(not(target_family = "wasm"))]
        let (
            prev_block_commitment,
            chain_commitment,
            nullifier_root,
            note_root,
            tx_commitment,
            proof_commitment,
            timestamp,
        ) = {
            let prev_block_commitment: Word = Word::new(rand_array());
            let chain_commitment = chain_commitment.unwrap_or(Word::new(rand_array()));
            let nullifier_root = Word::new(rand_array());
            let note_root = note_root.unwrap_or(Word::new(rand_array()));
            let tx_commitment = Word::new(rand_array());
            let proof_commitment = Word::new(rand_array());
            let timestamp = rand_value();

            (
                prev_block_commitment,
                chain_commitment,
                nullifier_root,
                note_root,
                tx_commitment,
                proof_commitment,
                timestamp,
            )
        };

        #[cfg(target_family = "wasm")]
        let (
            prev_block_commitment,
            chain_commitment,
            nullifier_root,
            note_root,
            tx_commitment,
            proof_commitment,
            timestamp,
        ) = {
            (
                Default::default(),
                chain_commitment.unwrap_or_default(),
                Default::default(),
                note_root.unwrap_or_default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )
        };

        BlockHeader::new(
            0,
            prev_block_commitment,
            block_num.into(),
            chain_commitment,
            account_root,
            nullifier_root,
            note_root,
            tx_commitment,
            tx_kernel_commitment,
            proof_commitment,
            timestamp,
        )
    }
}
