use miden_objects::{Felt, Word};
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

#[derive(Debug, Clone)]
struct EntryMetadata {
    map_ptr: u32,
    prev_item: u32,
    next_item: u32,
}

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

    fn head(&self) -> u32 {
        self.get_mem_value(self.map_ptr)
            .map(|head_ptr| head_ptr.try_into().expect("head ptr should be a valid ptr"))
            .unwrap_or(0)
    }

    fn key(&self, entry_ptr: u32) -> Word {
        self.get_mem_word(entry_ptr + 4).expect("entry pointer should be valid")
    }

    fn value(&self, entry_ptr: u32) -> Word {
        self.get_mem_word(entry_ptr + 8).expect("entry pointer should be valid")
    }

    fn metadata(&self, addr: u32) -> Result<EntryMetadata, LinkMapError> {
        let entry_metadata = self
            .get_mem_word(self.map_ptr)
            .ok_or_else(|| LinkMapError::InaccessibleMetadata(self.map_ptr))?;

        let map_ptr = entry_metadata[3];
        let map_ptr = map_ptr
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("map_ptr", map_ptr))?;

        let prev_item = entry_metadata[2];
        let prev_item = prev_item
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("prev_item", prev_item))?;

        let next_item = entry_metadata[1];
        let next_item = next_item
            .try_into()
            .map_err(|_| LinkMapError::PointerRangeExceeded("next_item", next_item))?;

        Ok(EntryMetadata { map_ptr, prev_item, next_item })
    }

    pub fn find(&self, key: Word) -> Result<Option<u32>, LinkMapError> {
        let mut entry = None;

        let mut current_entry = self.head();
        // loop {
        let entry_key = self.key(current_entry);
        if key == entry_key {
            return Ok(Some(current_entry));
        }

        // let mut current_entry_metadata = self.metadata(self.map_ptr)?;
        // }

        Ok(entry)
    }
}
