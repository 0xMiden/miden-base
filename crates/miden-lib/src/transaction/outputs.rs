use miden_objects::{
    AccountError, Felt, WORD_SIZE, Word, WordError,
    account::{AccountHeader, AccountId},
};

use super::memory::{
    ACCT_CODE_COMMITMENT_OFFSET, ACCT_DATA_MEM_SIZE, ACCT_ID_AND_NONCE_OFFSET, ACCT_NONCE_IDX,
    ACCT_STORAGE_COMMITMENT_OFFSET, ACCT_VAULT_ROOT_OFFSET, MemoryOffset,
};
use crate::transaction::memory::{ACCT_ID_PREFIX_IDX, ACCT_ID_SUFFIX_IDX};

// STACK OUTPUTS
// ================================================================================================

/// The index of the word at which the final account nonce is stored on the output stack.
pub const OUTPUT_NOTES_COMMITMENT_WORD_IDX: usize = 0;

/// The index of the word at which the account update commitment is stored on the output stack.
pub const ACCOUNT_UPDATE_COMMITMENT_WORD_IDX: usize = 1;

/// The index of the word at which the fee asset is stored on the output stack.
pub const FEE_ASSET_WORD_IDX: usize = 2;

/// The index of the item at which the expiration block height is stored on the output stack.
pub const EXPIRATION_BLOCK_ELEMENT_IDX: usize = 12;

// ACCOUNT HEADER EXTRACTOR
// ================================================================================================

/// Parses the account header data returned by the VM into individual account component commitments.
/// Returns a tuple of account ID, vault root, storage commitment, code commitment, and nonce.
pub fn parse_final_account_header(elements: &[Felt]) -> Result<AccountHeader, AccountError> {
    if elements.len() != ACCT_DATA_MEM_SIZE {
        return Err(AccountError::HeaderDataIncorrectLength {
            actual: elements.len(),
            expected: ACCT_DATA_MEM_SIZE,
        });
    }

    let id = AccountId::try_from([
        elements[ACCT_ID_AND_NONCE_OFFSET as usize + ACCT_ID_PREFIX_IDX],
        elements[ACCT_ID_AND_NONCE_OFFSET as usize + ACCT_ID_SUFFIX_IDX],
    ])
    .map_err(AccountError::FinalAccountHeaderIdParsingFailed)?;
    let nonce = elements[ACCT_ID_AND_NONCE_OFFSET as usize + ACCT_NONCE_IDX];
    let vault_root = parse_word(elements, ACCT_VAULT_ROOT_OFFSET)
        .expect("we should have sliced off exactly 4 bytes");
    let storage_commitment = parse_word(elements, ACCT_STORAGE_COMMITMENT_OFFSET)
        .expect("we should have sliced off exactly 4 bytes");
    let code_commitment = parse_word(elements, ACCT_CODE_COMMITMENT_OFFSET)
        .expect("we should have sliced off exactly 4 bytes");

    Ok(AccountHeader::new(id, nonce, vault_root, storage_commitment, code_commitment))
}

// HELPER FUNCTIONS
// ================================================================================================

/// Creates a new `Word` instance from the slice of `Felt`s using provided offset.
fn parse_word(data: &[Felt], offset: MemoryOffset) -> Result<Word, WordError> {
    Word::try_from(&data[offset as usize..offset as usize + WORD_SIZE])
}
