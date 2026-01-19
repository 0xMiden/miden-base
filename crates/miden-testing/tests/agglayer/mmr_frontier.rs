use alloc::format;
use alloc::string::ToString;

use miden_agglayer::agglayer_library;
use miden_crypto::hash::keccak::{Keccak256, Keccak256Digest};
use miden_protocol::Felt;
use miden_protocol::utils::sync::LazyLock;
use miden_standards::code_builder::CodeBuilder;
use miden_testing::TransactionContextBuilder;

// KECCAK MMR FRONTIER
// ================================================================================================

static CANONICAL_ZEROS_32: LazyLock<Vec<Keccak256Digest>> = LazyLock::new(|| {
    let mut zeros_by_height = Vec::with_capacity(32);

    // Push the zero of height 0 to the zeros vec. This is done separately because the zero of
    // height 0 is just a plain zero array ([0u8; 32]), it doesn't require to perform any hashing.
    zeros_by_height.push(Keccak256Digest::default());

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
    num_leaves: u32,
    frontier: [Keccak256Digest; TREE_HEIGHT],
}

impl<const TREE_HEIGHT: usize> KeccakMmrFrontier32<TREE_HEIGHT> {
    pub fn new() -> Self {
        Self {
            num_leaves: 0,
            frontier: [Keccak256Digest::default(); TREE_HEIGHT],
        }
    }

    pub fn append_and_update_frontier(&mut self, new_leaf: Keccak256Digest) -> Keccak256Digest {
        let mut curr_hash = new_leaf;
        let mut idx = self.num_leaves;
        self.num_leaves += 1;

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

// TESTS
// ================================================================================================

#[tokio::test]
async fn test_append_and_update_frontier() -> anyhow::Result<()> {
    let mut mmr_frontier = KeccakMmrFrontier32::<32>::new();

    let mut source = "use miden::agglayer::mmr_frontier32_keccak begin".to_string();

    for round in 0..32 {
        // construct the leaf from the hex representation of the round number
        let leaf = Keccak256Digest::try_from(format!("{:#066x}", round).as_str()).unwrap();
        let root = mmr_frontier.append_and_update_frontier(leaf);
        let num_leaves = mmr_frontier.num_leaves;

        source.push_str(&leaf_assertion_code(leaf, root, num_leaves));
    }

    source.push_str("end");

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

#[tokio::test]
async fn test_check_empty_mmr_root() -> anyhow::Result<()> {
    let zero_leaf = Keccak256Digest::default();
    let zero_31 = *CANONICAL_ZEROS_32.get(31).expect("zeros should have 32 values total");
    let empty_mmr_root = Keccak256::merge(&[zero_31, zero_31]);

    let mut source = "use miden::agglayer::mmr_frontier32_keccak begin".to_string();

    for round in 1..=32 {
        // check that pushing the zero leaves into the MMR doesn't change its root
        source.push_str(&leaf_assertion_code(zero_leaf, empty_mmr_root, round));
    }

    source.push_str("end");

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

// SOLIDITY COMPATIBILITY TESTS
// ================================================================================================
// These tests verify that the Rust KeccakMmrFrontier32 implementation produces identical
// results to the Solidity DepositContractBase.sol implementation.
// Test vectors generated from: https://github.com/agglayer/agglayer-contracts
// Commit: e468f9b0967334403069aa650d9f1164b1731ebb

/// Test vectors from Solidity DepositContractBase.sol
/// Each tuple is (leaf_hex, expected_root_hex, expected_count)
const SOLIDITY_TEST_VECTORS: &[(&str, &str, u32)] = &[
    ("0x0000000000000000000000000000000000000000000000000000000000000000", "0x27ae5ba08d7291c96c8cbddcc148bf48a6d68c7974b94356f53754ef6171d757", 1),
    ("0x0000000000000000000000000000000000000000000000000000000000000001", "0x4a90a2c108a29b7755a0a915b9bb950233ce71f8a01859350d7b73cc56f57a62", 2),
    ("0x0000000000000000000000000000000000000000000000000000000000000002", "0x2757cc260a62cc7c7708c387ea99f2a6bb5f034ed00da845734bec4d3fa3abfe", 3),
    ("0x0000000000000000000000000000000000000000000000000000000000000003", "0xcb305ccda4331eb3fd9e17b81a5a0b336fb37a33f927698e9fb0604e534c6a01", 4),
    ("0x0000000000000000000000000000000000000000000000000000000000000004", "0xa377a6262d3bae7be0ce09c2cc9f767b0f31848c268a4bdc12b63a451bb97281", 5),
    ("0x0000000000000000000000000000000000000000000000000000000000000005", "0x440213f4dff167e3f5c655fbb6a3327af3512affed50ce3c1a3f139458a8a6d1", 6),
    ("0x0000000000000000000000000000000000000000000000000000000000000006", "0xdd716d2905f2881005341ff1046ced5ee15cc63139716f56ed6be1d075c3f4a7", 7),
    ("0x0000000000000000000000000000000000000000000000000000000000000007", "0xd6ebf96fcc3344fa755057b148162f95a93491bc6e8be756d06ec64df4df90fc", 8),
    ("0x0000000000000000000000000000000000000000000000000000000000000008", "0x8b3bf2c95f3d0f941c109adfc3b652fadfeaf6f34be52524360a001cb151b5c9", 9),
    ("0x0000000000000000000000000000000000000000000000000000000000000009", "0x74a5712654eccd015c44aca31817fd8bee8da400ada986a78384ef3594f2d459", 10),
    ("0x000000000000000000000000000000000000000000000000000000000000000a", "0x95dd1209b92cce04311dfc8670b03428408c4ff62beb389e71847971f73702fa", 11),
    ("0x000000000000000000000000000000000000000000000000000000000000000b", "0x0a83f3b2a75e19b7255b1de379ea9a71aef9716a3aef20a86abe625f088bbebf", 12),
    ("0x000000000000000000000000000000000000000000000000000000000000000c", "0x601ba73b45858be76c8d02799fd70a5e1713e04031aa3be6746f95a17c343173", 13),
    ("0x000000000000000000000000000000000000000000000000000000000000000d", "0x93d741c47aa73e36d3c7697758843d6af02b10ed38785f367d1602c8638adb0d", 14),
    ("0x000000000000000000000000000000000000000000000000000000000000000e", "0x578f0d0a9b8ed5a4f86181b7e479da7ad72576ba7d3f36a1b72516aa0900c8ac", 15),
    ("0x000000000000000000000000000000000000000000000000000000000000000f", "0x995c30e6b58c6e00e06faf4b5c94a21eb820b9db7ad30703f8e3370c2af10c11", 16),
    ("0x0000000000000000000000000000000000000000000000000000000000000010", "0x49fb7257be1e954c377dc2557f5ca3f6fc7002d213f2772ab6899000e465236c", 17),
    ("0x0000000000000000000000000000000000000000000000000000000000000011", "0x06fee72550896c50e28b894c60a3132bfe670e5c7a77ab4bb6a8ffb4abcf9446", 18),
    ("0x0000000000000000000000000000000000000000000000000000000000000012", "0xbba3a807e79d33c6506cd5ecb5d50417360f8be58139f6dbe2f02c92e4d82491", 19),
    ("0x0000000000000000000000000000000000000000000000000000000000000013", "0x1243fbd4d21287dbdaa542fa18a6a172b60d1af2c517b242914bdf8d82a98293", 20),
    ("0x0000000000000000000000000000000000000000000000000000000000000014", "0x02b7b57e407fbccb506ed3199922d6d9bd0f703a1919d388c76867399ed44286", 21),
    ("0x0000000000000000000000000000000000000000000000000000000000000015", "0xa15e7890d8f860a2ef391f9f58602dec7027c19e8f380980f140bbb92a3e00ba", 22),
    ("0x0000000000000000000000000000000000000000000000000000000000000016", "0x2cb7eff4deb9bf6bbb906792bc152f1e63759b30e7829bfb5f3257ee600303f5", 23),
    ("0x0000000000000000000000000000000000000000000000000000000000000017", "0xb1b034b4784411dc6858a0da771acef31be60216be0520a7950d29f66aee1fc5", 24),
    ("0x0000000000000000000000000000000000000000000000000000000000000018", "0x3b17098f521ca0719e144a12bb79fdc51a3bc70385b5c2ee46b5762aae741f4f", 25),
    ("0x0000000000000000000000000000000000000000000000000000000000000019", "0xd3e054489aa750d41938143011666a83e5e6b1477cce5ad612447059c2d8b939", 26),
    ("0x000000000000000000000000000000000000000000000000000000000000001a", "0x6d15443ab2f39cce7fbe131843cdad6f27400eb179efb866569dd48baaf3ed4d", 27),
    ("0x000000000000000000000000000000000000000000000000000000000000001b", "0xf9386ef40320c369185e48132f8fbf2f3e78d9598495dd342bcf4f41388d460d", 28),
    ("0x000000000000000000000000000000000000000000000000000000000000001c", "0xb618ebe1f7675ef246a8cbb93519469076d5caacd4656330801537933e27b172", 29),
    ("0x000000000000000000000000000000000000000000000000000000000000001d", "0x6c8c90b5aa967c98061a2dd09ea74dfb61fd9e86e308f14453e9e0ae991116de", 30),
    ("0x000000000000000000000000000000000000000000000000000000000000001e", "0x06f51cfc733d71220d6e5b70a6b33a8d47a1ab55ac045fac75f26c762d7b29c9", 31),
    ("0x000000000000000000000000000000000000000000000000000000000000001f", "0x82d1ddf8c6d986dee7fc6fa2d7120592d1dc5026b1bb349fcc9d5c73ac026f56", 32),
];

/// Canonical zeros from Solidity DepositContractBase.sol
/// ZERO_n = keccak256(ZERO_{n-1} || ZERO_{n-1}), where ZERO_0 = 0x00...00
const SOLIDITY_CANONICAL_ZEROS: &[&str] = &[
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    "0xad3228b676f7d3cd4284a5443f17f1962b36e491b30a40b2405849e597ba5fb5",
    "0xb4c11951957c6f8f642c4af61cd6b24640fec6dc7fc607ee8206a99e92410d30",
    "0x21ddb9a356815c3fac1026b6dec5df3124afbadb485c9ba5a3e3398a04b7ba85",
    "0xe58769b32a1beaf1ea27375a44095a0d1fb664ce2dd358e7fcbfb78c26a19344",
    "0x0eb01ebfc9ed27500cd4dfc979272d1f0913cc9f66540d7e8005811109e1cf2d",
    "0x887c22bd8750d34016ac3c66b5ff102dacdd73f6b014e710b51e8022af9a1968",
    "0xffd70157e48063fc33c97a050f7f640233bf646cc98d9524c6b92bcf3ab56f83",
    "0x9867cc5f7f196b93bae1e27e6320742445d290f2263827498b54fec539f756af",
    "0xcefad4e508c098b9a7e1d8feb19955fb02ba9675585078710969d3440f5054e0",
    "0xf9dc3e7fe016e050eff260334f18a5d4fe391d82092319f5964f2e2eb7c1c3a5",
    "0xf8b13a49e282f609c317a833fb8d976d11517c571d1221a265d25af778ecf892",
    "0x3490c6ceeb450aecdc82e28293031d10c7d73bf85e57bf041a97360aa2c5d99c",
    "0xc1df82d9c4b87413eae2ef048f94b4d3554cea73d92b0f7af96e0271c691e2bb",
    "0x5c67add7c6caf302256adedf7ab114da0acfe870d449a3a489f781d659e8becc",
    "0xda7bce9f4e8618b6bd2f4132ce798cdc7a60e7e1460a7299e3c6342a579626d2",
    "0x2733e50f526ec2fa19a22b31e8ed50f23cd1fdf94c9154ed3a7609a2f1ff981f",
    "0xe1d3b5c807b281e4683cc6d6315cf95b9ade8641defcb32372f1c126e398ef7a",
    "0x5a2dce0a8a7f68bb74560f8f71837c2c2ebbcbf7fffb42ae1896f13f7c7479a0",
    "0xb46a28b6f55540f89444f63de0378e3d121be09e06cc9ded1c20e65876d36aa0",
    "0xc65e9645644786b620e2dd2ad648ddfcbf4a7e5b1a3a4ecfe7f64667a3f0b7e2",
    "0xf4418588ed35a2458cffeb39b93d26f18d2ab13bdce6aee58e7b99359ec2dfd9",
    "0x5a9c16dc00d6ef18b7933a6f8dc65ccb55667138776f7dea101070dc8796e377",
    "0x4df84f40ae0c8229d0d6069e5c8f39a7c299677a09d367fc7b05e3bc380ee652",
    "0xcdc72595f74c7b1043d0e1ffbab734648c838dfb0527d971b602bc216c9619ef",
    "0x0abf5ac974a1ed57f4050aa510dd9c74f508277b39d7973bb2dfccc5eeb0618d",
    "0xb8cd74046ff337f0a7bf2c8e03e10f642c1886798d71806ab1e888d9e5ee87d0",
    "0x838c5655cb21c6cb83313b5a631175dff4963772cce9108188b34ac87c81c41e",
    "0x662ee4dd2dd7b2bc707961b1e646c4047669dcb6584f0d8d770daf5d7e7deb2e",
    "0x388ab20e2573d171a88108e79d820e98f26c0b84aa8b2f4aa4968dbb818ea322",
    "0x93237c50ba75ee485f4c22adf2f741400bdf8d6a9cc7df7ecae576221665d735",
    "0x8448818bb4ae4562849e949e17ac16e0be16688e156b5cf15e098c627c0056a9",
];

/// Verifies that the Rust KeccakMmrFrontier32 produces the same canonical zeros as Solidity.
#[test]
fn test_solidity_canonical_zeros_compatibility() {
    for (height, expected_hex) in SOLIDITY_CANONICAL_ZEROS.iter().enumerate() {
        let expected = Keccak256Digest::try_from(*expected_hex).unwrap();
        let actual = CANONICAL_ZEROS_32[height];

        assert_eq!(
            actual, expected,
            "Canonical zero mismatch at height {}: expected {}, got {:?}",
            height, expected_hex, actual
        );
    }
}

/// Verifies that the Rust KeccakMmrFrontier32 produces the same roots as Solidity's
/// DepositContractBase after adding each leaf.
#[test]
fn test_solidity_mmr_frontier_compatibility() {
    let mut mmr_frontier = KeccakMmrFrontier32::<32>::new();

    for (leaf_hex, expected_root_hex, expected_count) in SOLIDITY_TEST_VECTORS.iter() {
        let leaf = Keccak256Digest::try_from(*leaf_hex).unwrap();
        let expected_root = Keccak256Digest::try_from(*expected_root_hex).unwrap();

        let actual_root = mmr_frontier.append_and_update_frontier(leaf);
        let actual_count = mmr_frontier.num_leaves;

        assert_eq!(
            actual_count, *expected_count,
            "Leaf count mismatch after adding leaf {}: expected {}, got {}",
            leaf_hex, expected_count, actual_count
        );

        assert_eq!(
            actual_root, expected_root,
            "Root mismatch after adding leaf {} (count={}): expected {}, got {:?}",
            leaf_hex, expected_count, expected_root_hex, actual_root
        );
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Transforms the `[Keccak256Digest]` into two word strings: (`a, b, c, d`, `e, f, g, h`)
fn keccak_digest_to_word_strings(digest: Keccak256Digest) -> (String, String) {
    let double_word = (*digest)
        .chunks(4)
        .map(|chunk| Felt::from(u32::from_le_bytes(chunk.try_into().unwrap())).to_string())
        .rev()
        .collect::<Vec<_>>();

    (double_word[0..4].join(", "), double_word[4..8].join(", "))
}

fn leaf_assertion_code(
    leaf: Keccak256Digest,
    expected_root: Keccak256Digest,
    num_leaves: u32,
) -> String {
    let (leaf_hi, leaf_lo) = keccak_digest_to_word_strings(leaf);
    let (root_hi, root_lo) = keccak_digest_to_word_strings(expected_root);

    format!(
        r#"
            # load the provided leaf onto the stack
            push.[{leaf_hi}]
            push.[{leaf_lo}]

            # add this leaf to the MMR frontier
            exec.mmr_frontier32_keccak::append_and_update_frontier
            # => [NEW_ROOT_LO, NEW_ROOT_HI, new_leaf_count]

            # assert the root correctness after the first leaf was added
            push.[{root_lo}]
            push.[{root_hi}]
            movdnw.3
            # => [EXPECTED_ROOT_LO, NEW_ROOT_LO, NEW_ROOT_HI, EXPECTED_ROOT_HI, new_leaf_count]

            assert_eqw.err="MMR root (LO) is incorrect"
            # => [NEW_ROOT_HI, EXPECTED_ROOT_HI, new_leaf_count]

            assert_eqw.err="MMR root (HI) is incorrect"
            # => [new_leaf_count]

            # assert the new number of leaves
            push.{num_leaves}
            assert_eq.err="new leaf count is incorrect"
        "#
    )
}
