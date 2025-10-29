use core::ops::{Deref, DerefMut};

use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use miden_crypto::dsa::ecdsa_k256_keccak::{PublicKey, Signature};

use crate::block::{BlockHeader, ProposedBlock};

#[derive(Debug, Clone)]
pub struct SignedBlock {
    header: BlockHeader,
    proposed_block: ProposedBlock,
    signature: Signature,
}

impl Deref for SignedBlock {
    type Target = ProposedBlock;

    fn deref(&self) -> &Self::Target {
        &self.proposed_block
    }
}

impl DerefMut for SignedBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.proposed_block
    }
}

impl SignedBlock {
    /// Creates a new signed block with the given proven block and signature.
    ///
    /// This should only be used internally by the [`ProvenBlock`] struct.
    pub(crate) fn new(
        header: BlockHeader,
        proposed_block: ProposedBlock,
        signature: Signature,
    ) -> Self {
        SignedBlock { header, proposed_block, signature }
    }

    /// Returns a reference to the signature of the signed block.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Verifies the signature of the signed block.
    pub fn verify(&self, pub_key: &PublicKey) -> bool {
        self.signature.verify(self.header.commitment(), pub_key)
    }

    /// Consumes the signed block and returns its parts.
    pub fn into_parts(self) -> (BlockHeader, ProposedBlock, Signature) {
        (self.header, self.proposed_block, self.signature)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for SignedBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.header.write_into(target);
        self.proposed_block.write_into(target);
        self.signature.write_into(target);
    }
}

impl Deserializable for SignedBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            header: BlockHeader::read_from(source)?,
            proposed_block: ProposedBlock::read_from(source)?,
            signature: Signature::read_from(source)?,
        };
        Ok(block)
    }
}

// TESTING
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::vec;

    use miden_crypto::dsa::ecdsa_k256_keccak::SecretKey;

    use crate::Word;
    use crate::account::AccountId;
    use crate::block::{BlockHeader, BlockNumber, FeeParameters, ProvenBlock};
    use crate::testing::account_id::ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET;
    use crate::transaction::OrderedTransactionHeaders;

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Creates a mock ProvenBlock for testing.
    fn create_mock_proven_block() -> ProvenBlock {
        let native_asset_id = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();
        let fee_params = FeeParameters::new(native_asset_id, 1000).unwrap();

        let header = BlockHeader::new(
            1,                    // version
            Word::default(),      // prev_block_commitment
            BlockNumber::GENESIS, // block_num
            Word::default(),      // chain_commitment
            Word::default(),      // account_root
            Word::default(),      // nullifier_root
            Word::default(),      // note_root
            Word::default(),      // tx_commitment
            Word::default(),      // proof_commitment
            fee_params,           // fee_parameters
            0,                    // timestamp
        );

        ProvenBlock::new_unchecked(
            header,
            vec![],
            vec![],
            vec![],
            OrderedTransactionHeaders::new_unchecked(vec![]),
            Word::default(),
        )
    }

    /// Creates a mock SecretKey for testing.
    fn create_mock_secret_key() -> SecretKey {
        SecretKey::new()
    }

    // TESTS
    // --------------------------------------------------------------------------------------------

    // #[test]
    // fn test_proven_block_sign_creates_valid_signed_block() {
    //     // Prepare and sign a block.
    //     let proven_block = create_mock_proven_block();
    //     let commitment = proven_block.commitment();
    //     let mut secret_key = create_mock_secret_key();
    //     let public_key = secret_key.public_key();
    //     let signed_block = proven_block.sign(&mut secret_key);

    //     // Assert correctness.
    //     assert_eq!(
    //         signed_block.commitment(),
    //         commitment,
    //         "Signed block commitment does not match original commitment"
    //     );
    //     assert_eq!(
    //         signed_block.header().block_num(),
    //         BlockNumber::GENESIS,
    //         "Block number should be genesis"
    //     );
    //     assert!(
    //         !signed_block.signature().to_bytes().iter().all(|&b| b == 0),
    //         "Signature should be non-zero"
    //     );
    //     assert!(signed_block.verify(&public_key), "Signature verification failed");
    // }

    // #[test]
    // fn test_signed_block_verify_with_incorrect_public_key() {
    //     // Prepare and sign block.
    //     let proven_block = create_mock_proven_block();
    //     let mut secret_key1 = create_mock_secret_key();
    //     let secret_key2 = create_mock_secret_key();
    //     let wrong_public_key = secret_key2.public_key();
    //     let signed_block = proven_block.sign(&mut secret_key1);

    //     // Assert incorrectness.
    //     assert!(
    //         !signed_block.verify(&wrong_public_key),
    //         "Signature verification should fail with incorrect public key"
    //     );
    // }
}
