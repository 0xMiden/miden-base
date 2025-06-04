use core::cmp::Ordering;

use miden_objects::{Digest, Felt, Word};
use vm_processor::{ContextId, ProcessState};

#[derive(Debug, thiserror::Error)]
pub enum LinkMapError {
    #[error("link map pointer {0} exceeds u32 range {1}")]
    PointerRangeExceeded(&'static str, Felt),
    #[error("metadata for entry pointer {0} has not been initialized")]
    InaccessibleMetadata(u32),
    #[error("provided pointer {0} is not word-aligned")]
    UnalignedMemoryAddress(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct LinkMap<'process> {
    map_ptr: u32,
    process: ProcessState<'process>,
}

impl<'process> LinkMap<'process> {
    pub fn new(map_ptr: Felt, process: ProcessState<'process>) -> Result<Self, LinkMapError> {
        let map_ptr = map_ptr
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("map_ptr", map_ptr))?;
        if map_ptr % 4 != 0 {
            return Err(LinkMapError::UnalignedMemoryAddress(map_ptr));
        }

        Ok(Self { map_ptr, process })
    }

    fn get_mem_value(&self, addr: u32) -> Option<Felt> {
        self.process.get_mem_value(ContextId::root(), addr)
    }

    fn get_mem_word(&self, addr: u32) -> Option<Word> {
        self.process
            .get_mem_word(ContextId::root(), addr)
            .expect("address should be word-aligned")
    }

    pub fn is_empty(&self) -> bool {
        self.get_mem_value(self.map_ptr).is_none()
    }

    fn head(&self) -> Option<u32> {
        self.get_mem_value(self.map_ptr)
            .map(|head_ptr| head_ptr.try_into().expect("head ptr should be a valid ptr"))
    }

    pub fn entry(&self, entry_ptr: u32) -> Entry {
        let key = self.key(entry_ptr);
        let value = self.value(entry_ptr);
        let metadata = self.metadata(entry_ptr);

        Entry { ptr: entry_ptr, metadata, key, value }
    }

    fn key(&self, entry_ptr: u32) -> Word {
        self.get_mem_word(entry_ptr + 4).expect("entry pointer should be valid")
    }

    fn value(&self, entry_ptr: u32) -> Word {
        self.get_mem_word(entry_ptr + 8).expect("entry pointer should be valid")
    }

    fn metadata(&self, entry_ptr: u32) -> EntryMetadata {
        let entry_metadata = self
            .get_mem_word(entry_ptr)
            .ok_or_else(|| LinkMapError::InaccessibleMetadata(entry_ptr))
            .unwrap();

        let map_ptr = entry_metadata[0];
        let map_ptr = map_ptr
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("map_ptr", map_ptr))
            .unwrap();

        let prev_item = entry_metadata[1];
        let prev_item = prev_item
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("prev_item", prev_item))
            .unwrap();

        let next_item = entry_metadata[2];
        let next_item = next_item
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("next_item", next_item))
            .unwrap();

        EntryMetadata { map_ptr, prev_item, next_item }
    }

    pub fn find_insertion(&self, key: Word) -> (Operation, u32) {
        let Some(current_head) = self.head() else {
            return (Operation::InsertAtHead, 0);
        };

        let mut last_entry_ptr: u32 = current_head;

        for entry in self.iter() {
            match Digest::from(key).cmp(&Digest::from(entry.key)) {
                Ordering::Equal => {
                    return (Operation::Update, entry.ptr);
                },
                Ordering::Less => {
                    if entry.ptr == current_head {
                        return (Operation::InsertAtHead, entry.ptr);
                    }

                    break;
                },
                Ordering::Greater => {
                    std::println!("{key:?} > {:?}", entry.key);
                    std::println!("set last_entry_ptr = {}", entry.ptr);
                    last_entry_ptr = entry.ptr;
                },
            }
        }

        (Operation::InsertAfterEntry, last_entry_ptr)
    }

    pub fn find(&self, key: Word) -> Option<u32> {
        self.iter()
            .find_map(|entry| if entry.key == key { Some(entry.ptr) } else { None })
    }

    pub fn iter(&self) -> LinkMapIter {
        LinkMapIter {
            current_entry_ptr: self.head().unwrap_or(0),
            map: *self,
        }
    }

    pub fn compare_keys(key0: Word, key1: Word) -> Ordering {
        key0.iter()
            .rev()
            .map(Felt::as_int)
            .zip(key1.iter().rev().map(Felt::as_int))
            .fold(Ordering::Equal, |ord, (felt0, felt1)| match ord {
                Ordering::Equal => felt0.cmp(&felt1),
                _ => ord,
            })
    }
}

pub struct LinkMapIter<'process> {
    current_entry_ptr: u32,
    map: LinkMap<'process>,
}

impl<'process> Iterator for LinkMapIter<'process> {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_entry_ptr == 0 {
            return None;
        }

        let current_entry = self.map.entry(self.current_entry_ptr);

        self.current_entry_ptr = current_entry.metadata.next_item;

        Some(current_entry)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub ptr: u32,
    pub metadata: EntryMetadata,
    pub key: Word,
    pub value: Word,
}

#[derive(Debug, Clone, Copy)]
pub struct EntryMetadata {
    pub map_ptr: u32,
    pub prev_item: u32,
    pub next_item: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Operation {
    Update = 0,
    InsertAtHead = 1,
    InsertAfterEntry = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_word(ints: [u32; 4]) -> Word {
        ints.map(|int| Felt::from(int))
    }

    #[test]
    fn compare_keys() {
        for (expected, key0, key1) in [
            (Ordering::Equal, [0, 0, 0, 0u32], [0, 0, 0, 0u32]),
            (Ordering::Greater, [1, 0, 0, 0u32], [0, 0, 0, 0u32]),
            (Ordering::Greater, [0, 1, 0, 0u32], [0, 0, 0, 0u32]),
            (Ordering::Greater, [0, 0, 1, 0u32], [0, 0, 0, 0u32]),
            (Ordering::Greater, [0, 0, 0, 1u32], [0, 0, 0, 0u32]),
            (Ordering::Less, [0, 0, 0, 0u32], [1, 0, 0, 0u32]),
            (Ordering::Less, [0, 0, 0, 0u32], [0, 1, 0, 0u32]),
            (Ordering::Less, [0, 0, 0, 0u32], [0, 0, 1, 0u32]),
            (Ordering::Less, [0, 0, 0, 0u32], [0, 0, 0, 1u32]),
            (Ordering::Greater, [0, 0, 0, 1u32], [1, 1, 1, 0u32]),
            (Ordering::Greater, [0, 0, 1, 0u32], [1, 1, 0, 0u32]),
            (Ordering::Less, [1, 1, 1, 0u32], [0, 0, 0, 1u32]),
            (Ordering::Less, [1, 1, 0, 0u32], [0, 0, 1, 0u32]),
        ] {
            assert_eq!(LinkMap::compare_keys(to_word(key0), to_word(key1)), expected);
        }
    }
}
