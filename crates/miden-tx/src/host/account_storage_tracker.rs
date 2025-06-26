use alloc::collections::BTreeMap;

use miden_objects::{Word, account::AccountStorageHeader};

#[derive(Debug, Clone)]
pub struct AccountInitStorageTracker {
    header: AccountStorageHeader,
    maps: BTreeMap<u8, Word>,
}

impl AccountInitStorageTracker {
    pub fn new(header: AccountStorageHeader) -> Self {
        Self { header, maps: BTreeMap::new() }
    }

    pub fn storage_header(&self) -> &AccountStorageHeader {
        &self.header
    }
}
