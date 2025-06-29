use super::{Digest, Felt};
use alloc::vec::Vec;

pub trait Committable {
    fn commitment(&self) -> Digest;

    fn to_elements(&self) -> Vec<Felt>;
}
