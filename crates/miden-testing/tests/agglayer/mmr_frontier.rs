use alloc::format;
use alloc::string::ToString;

use miden_agglayer::agglayer_library;
use miden_crypto::hash::keccak::{Keccak256, Keccak256Digest};
use miden_protocol::Felt;
use miden_protocol::utils::sync::LazyLock;
use miden_standards::code_builder::CodeBuilder;
use miden_testing::TransactionContextBuilder;

static CANONICAL_ZEROS_32: LazyLock<Vec<Keccak256Digest>> = LazyLock::new(|| {
    let mut zeros_by_height = Vec::with_capacity(32);

    // Push the zero of height 0 to the zeros vec. This is done separately because it requires
    // `Keccak256::hash` instead of `Keccak256::merge`
    zeros_by_height.push(Keccak256::hash(&[0u8; 32]));

    // Compute the canonical zeros for each height from 1 to 32
    // Zero of height `n` is computed as: `ZERO_N = Keccak256::merge(ZERO_{N-1}, ZERO_{N-1})`
    for _ in 1..32 {
        let last_zero = zeros_by_height.last().expect("zeros vec should have at least one value");
        let current_height_zero = Keccak256::merge(&[*last_zero, *last_zero]);
        zeros_by_height.push(current_height_zero);
    }

    zeros_by_height
});

struct KeccakMmrFrontier32<const TREE_HEIGHT: usize = 32> {
    leaves_num: u32,
    frontier: [Keccak256Digest; TREE_HEIGHT],
}

impl<const TREE_HEIGHT: usize> KeccakMmrFrontier32<TREE_HEIGHT> {
    pub fn new() -> Self {
        Self {
            leaves_num: 0,
            frontier: [Keccak256Digest::default(); TREE_HEIGHT],
        }
    }

    pub fn append_and_update_frontier(&mut self, new_leaf: Keccak256Digest) -> Keccak256Digest {
        let mut curr_hash = new_leaf;
        let mut idx = self.leaves_num;
        self.leaves_num += 1;

        for height in 0..TREE_HEIGHT {
            if (idx & 1) == 0 {
                // This height wasn't "occupied" yet: store cur as the subtree root at height h.
                self.frontier[height] = curr_hash;

                // Pair it with the canonical zero subtree on the right at this height.
                curr_hash = Keccak256::merge(&[curr_hash, CANONICAL_ZEROS_32[height]]);
            } else {
                // This height already had a subtree root stored in frontier[h], merge into parent.
                curr_hash = Keccak256::merge(&[self.frontier[height], curr_hash])
            }

            idx >>= 1;
        }

        // curr_hash at this point is equal to the root of the full tree
        curr_hash
    }
}

#[tokio::test]
async fn test_append_and_update_frontier() -> anyhow::Result<()> {
    let mut mmr_frontier = KeccakMmrFrontier32::<32>::new();

    // create a leaf from a random hex
    let first_leaf = Keccak256Digest::try_from(
        "0x110527e2a134fcb367f3bc770acc0d75b9c47bb4c5a78f0de02c80143340df62",
    )
    .unwrap();
    let first_root = mmr_frontier.append_and_update_frontier(first_leaf);
    let first_leaf_count = mmr_frontier.leaves_num;

    let second_leaf = Keccak256Digest::try_from(
        "0xa623afa60853762a72f9de96574ebee588ba5653cd2bd5e611e288f8da8c06b4",
    )
    .unwrap();
    let second_root = mmr_frontier.append_and_update_frontier(second_leaf);
    let second_leaf_count = mmr_frontier.leaves_num;

    let third_leaf = Keccak256Digest::try_from(
        "0x74a0e9822d944966bc7cfe38fc8af7dcd39a37f29750d11012931cef68a488d1",
    )
    .unwrap();
    let third_root = mmr_frontier.append_and_update_frontier(third_leaf);
    let third_leaf_count = mmr_frontier.leaves_num;

    let source = format!(
        r#"
        use miden::agglayer::collections::mmr_frontier32_keccak

        begin
            # assert first leaf, root and leaves count
            {first_assert}

            # assert second leaf, root and leaves count
            {second_assert}

            # assert third leaf, root and leaves count
            {third_assert}
        end
        "#,
        first_assert = leaf_assertion_code(first_leaf, first_root, first_leaf_count),
        second_assert = leaf_assertion_code(second_leaf, second_root, second_leaf_count),
        third_assert = leaf_assertion_code(third_leaf, third_root, third_leaf_count),
    );

    let tx_script = CodeBuilder::new()
        .with_statically_linked_library(&agglayer_library())?
        .compile_tx_script(source)?;
    TransactionContextBuilder::with_existing_mock_account()
        .tx_script(tx_script.clone())
        .build()?
        .execute()
        .await?;

    Ok(())
}

// HELPER FUNCTIONS
// ================================================================================================

fn keccak_digest_to_felt_strings(digest: Keccak256Digest) -> String {
    (*digest)
        .chunks(4)
        .map(|chunk| Felt::from(u32::from_le_bytes(chunk.try_into().unwrap())).to_string())
        .rev()
        .collect::<Vec<_>>()
        .join(".")
}

fn leaf_assertion_code(leaf: Keccak256Digest, root: Keccak256Digest, leaves_num: u32) -> String {
    format!(
        r#"
            # load the provided leaf onto the stack
            push.{LEAF}

            # add this leaf to the MMR frontier
            exec.mmr_frontier32_keccak::append_and_update_frontier
            # => [NEW_ROOT_1_LO, NEW_ROOT_1_HI, new_leaf_count=1]

            # assert the root correctness after the first leaf was added
            push.{ROOT}
            swapw movdnw.3
            # => [EXPECTED_ROOT_1_LO, NEW_ROOT_1_LO, NEW_ROOT_1_HI, EXPECTED_ROOT_1_HI, new_leaf_count=1]

            assert_eqw.err="MMR root (LO) after first leaf was added is incorrect"
            # => [NEW_ROOT_1_HI, EXPECTED_ROOT_1_HI, new_leaf_count=1]

            assert_eqw.err="MMR root (HI) after first leaf was added is incorrect"
            # => [new_leaf_count=1]

            # assert the new leaf count
            push.{leaves_num}
            assert_eq.err="first leaf count is incorrect"
        "#,
        LEAF = keccak_digest_to_felt_strings(leaf),
        ROOT = keccak_digest_to_felt_strings(root),
    )
}
