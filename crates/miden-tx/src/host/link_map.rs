use core::cmp::Ordering;

use miden_objects::{Felt, Word, assembly::mast::MastNodeExt};
use vm_processor::{
    AdviceProvider, AdviceSource, ContextId, ErrorContext, ExecutionError, ProcessState,
};

/// A map based on a sorted linked list.
///
/// This type enables access to the list in kernel memory.
///
/// See link_map.masm for docs.
///
/// # Warning
///
/// The functions on this type assume that the provided map_ptr points to a valid map in the
/// provided process. If those assumptions are violated, the functions may panic.
#[derive(Debug, Clone, Copy)]
pub struct LinkMap<'process> {
    map_ptr: u32,
    process: ProcessState<'process>,
}

impl<'process> LinkMap<'process> {
    /// Creates a new link map from the provided map_ptr in the provided process.
    pub fn new(map_ptr: Felt, process: ProcessState<'process>) -> Self {
        let map_ptr = map_ptr.try_into().expect("map_ptr must be a valid u32");
        if map_ptr % 4 != 0 {
            panic!("map_ptr must be word-aligned")
        }

        Self { map_ptr, process }
    }

    fn get_kernel_mem_value(&self, addr: u32) -> Option<Felt> {
        self.process.get_mem_value(ContextId::root(), addr)
    }

    fn get_kernel_mem_word(&self, addr: u32) -> Option<Word> {
        self.process
            .get_mem_word(ContextId::root(), addr)
            .expect("address should be word-aligned")
    }

    /// Handles a `LINK_MAP_SET_EVENT` emitted from a VM.
    ///
    /// Expected operand stack state before: [map_ptr, KEY, NEW_VALUE]
    /// Advice stack state after: [set_operation, entry_ptr]
    pub fn handle_set_event(
        process: ProcessState<'_>,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
        advice_provider: &mut impl AdviceProvider,
    ) -> Result<(), ExecutionError> {
        let map_ptr = process.get_stack_item(0);
        let map_key = [
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ];

        let link_map = LinkMap::new(map_ptr, process);

        let (set_op, entry_ptr) = link_map.compute_set_operation(map_key);

        advice_provider.push_stack(AdviceSource::Value(Felt::from(set_op as u8)), err_ctx)?;
        advice_provider.push_stack(AdviceSource::Value(Felt::from(entry_ptr)), err_ctx)?;

        Ok(())
    }

    /// Handles a `LINK_MAP_GET_EVENT` emitted from a VM.
    ///
    /// Expected operand stack state before: [map_ptr, KEY]
    /// Advice stack state after: [get_operation, entry_ptr]
    pub fn handle_get_event(
        process: ProcessState<'_>,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
        advice_provider: &mut impl AdviceProvider,
    ) -> Result<(), ExecutionError> {
        let map_ptr = process.get_stack_item(0);
        let map_key = [
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ];

        let link_map = LinkMap::new(map_ptr, process);
        let (get_op, entry_ptr) = link_map.compute_get_operation(map_key);

        advice_provider.push_stack(AdviceSource::Value(Felt::from(get_op as u8)), err_ctx)?;
        advice_provider.push_stack(AdviceSource::Value(Felt::from(entry_ptr)), err_ctx)?;

        Ok(())
    }

    /// Returns `true` if the map is empty, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.get_kernel_mem_value(self.map_ptr).is_none()
    }

    /// Returns the entry pointer at the head of the map.
    fn head(&self) -> Option<u32> {
        self.get_kernel_mem_value(self.map_ptr)
            .map(|head_ptr| head_ptr.try_into().expect("head ptr should be a valid ptr"))
    }

    fn entry(&self, entry_ptr: u32) -> Entry {
        let key = self.key(entry_ptr);
        let value = self.value(entry_ptr);
        let metadata = self.metadata(entry_ptr);

        Entry { ptr: entry_ptr, metadata, key, value }
    }

    fn key(&self, entry_ptr: u32) -> Word {
        self.get_kernel_mem_word(entry_ptr + 4).expect("entry pointer should be valid")
    }

    fn value(&self, entry_ptr: u32) -> Word {
        self.get_kernel_mem_word(entry_ptr + 8).expect("entry pointer should be valid")
    }

    fn metadata(&self, entry_ptr: u32) -> EntryMetadata {
        let entry_metadata =
            self.get_kernel_mem_word(entry_ptr).expect("entry pointer should be valid");

        let map_ptr = entry_metadata[0];
        let map_ptr = map_ptr.try_into().expect("entry_ptr should point to a u32 map_ptr");

        let prev_entry_ptr = entry_metadata[1];
        let prev_entry_ptr = prev_entry_ptr
            .try_into()
            .expect("entry_ptr should point to a u32 prev_entry_ptr");

        let next_entry_ptr = entry_metadata[2];
        let next_entry_ptr = next_entry_ptr
            .try_into()
            .expect("entry_ptr should point to a u32 next_entry_ptr");

        EntryMetadata { map_ptr, prev_entry_ptr, next_entry_ptr }
    }

    fn compute_set_operation(&self, key: Word) -> (SetOperation, u32) {
        let Some(current_head) = self.head() else {
            return (SetOperation::InsertAtHead, 0);
        };

        let mut last_entry_ptr: u32 = current_head;

        for entry in self.iter() {
            match Self::compare_keys(key, entry.key) {
                Ordering::Equal => {
                    return (SetOperation::Update, entry.ptr);
                },
                Ordering::Less => {
                    if entry.ptr == current_head {
                        return (SetOperation::InsertAtHead, entry.ptr);
                    }

                    break;
                },
                Ordering::Greater => {
                    last_entry_ptr = entry.ptr;
                },
            }
        }

        (SetOperation::InsertAfterEntry, last_entry_ptr)
    }

    fn compute_get_operation(&self, key: Word) -> (GetOperation, u32) {
        let (set_op, entry_ptr) = self.compute_set_operation(key);
        let get_op = match set_op {
            SetOperation::Update => GetOperation::Found,
            SetOperation::InsertAtHead => GetOperation::AbsentAtHead,
            SetOperation::InsertAfterEntry => GetOperation::AbsentAfterEntry,
        };
        (get_op, entry_ptr)
    }

    /// Returns an iterator over the link map entries.
    pub fn iter(&self) -> impl Iterator<Item = Entry> {
        LinkMapIter {
            current_entry_ptr: self.head().unwrap_or(0),
            map: *self,
        }
    }

    /// Compares key0 with key1 by comparing the individual felts from most significant (index 3) to
    /// least significant (index 0).
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

/// An iterator over a [`LinkMap`].
struct LinkMapIter<'process> {
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

        self.current_entry_ptr = current_entry.metadata.next_entry_ptr;

        Some(current_entry)
    }
}

/// An entry in a [`LinkMap`].
///
/// Exposed for testing purposes only.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub ptr: u32,
    pub metadata: EntryMetadata,
    pub key: Word,
    pub value: Word,
}

/// An entry's metadata in a [`LinkMap`].
///
/// Exposed for testing purposes only.
#[derive(Debug, Clone, Copy)]
pub struct EntryMetadata {
    pub map_ptr: u32,
    pub prev_entry_ptr: u32,
    pub next_entry_ptr: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum GetOperation {
    Found = 0,
    AbsentAtHead = 1,
    AbsentAfterEntry = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum SetOperation {
    Update = 0,
    InsertAtHead = 1,
    InsertAfterEntry = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_word(ints: [u32; 4]) -> Word {
        ints.map(Felt::from)
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
