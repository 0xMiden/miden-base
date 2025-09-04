use miden_objects::Word;
use miden_objects::crypto::dsa::rpo_falcon512::PublicKey;

use super::auth::AuthScheme;

pub mod auth;
pub mod components;
pub mod faucets;
pub mod interface;
pub mod wallets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKeyCommitment(pub Word);

impl PublicKeyCommitment {
    pub fn empty() -> Self {
        Self(Word::empty())
    }
}

impl From<PublicKey> for PublicKeyCommitment {
    fn from(value: PublicKey) -> Self {
        Self(value.to_commitment())
    }
}

impl From<PublicKeyCommitment> for Word {
    fn from(value: PublicKeyCommitment) -> Self {
        value.0
    }
}

impl From<Word> for PublicKeyCommitment {
    fn from(value: Word) -> Self {
        Self(value)
    }
}
