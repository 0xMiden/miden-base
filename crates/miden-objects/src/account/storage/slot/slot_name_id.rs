use core::cmp::Ordering;

use crate::Felt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotNameId {
    prefix: Felt,
    suffix: Felt,
}

impl SlotNameId {
    pub fn new(prefix: Felt, suffix: Felt) -> Self {
        Self { prefix, suffix }
    }

    pub fn prefix(&self) -> Felt {
        self.prefix
    }

    pub fn suffix(&self) -> Felt {
        self.suffix
    }
}

impl Ord for SlotNameId {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.prefix.as_int().cmp(&other.prefix.as_int()) {
            ord @ Ordering::Less | ord @ Ordering::Greater => ord,
            Ordering::Equal => self.suffix.as_int().cmp(&other.suffix.as_int()),
        }
    }
}

impl PartialOrd for SlotNameId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
