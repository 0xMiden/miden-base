use alloc::vec::Vec;
use core::cmp::Ordering;

use miden_objects::{Felt, LexicographicWord, Word, ZERO};
use vm_processor::{AdviceMutation, ContextId, EventError, ProcessState};

// LINK MAP
// ================================================================================================

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
    process: &'process ProcessState<'process>,
}

impl<'process> LinkMap<'process> {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Creates a new link map from the provided map_ptr in the provided process.
    pub fn new(map_ptr: Felt, process: &'process ProcessState<'process>) -> Self {
        let map_ptr: u32 = map_ptr.try_into().expect("map_ptr must be a valid u32");

        Self { map_ptr, process }
    }

    // PUBLIC METHODS
    // --------------------------------------------------------------------------------------------

    /// Handles a `LINK_MAP_SET_EVENT` emitted from a VM.
    ///
    /// Expected operand stack state before: [map_ptr, KEY, NEW_VALUE]
    /// Advice stack state after: [set_operation, entry_ptr]
    pub fn handle_set_event(process: &ProcessState<'_>) -> Result<Vec<AdviceMutation>, EventError> {
        let map_ptr = process.get_stack_item(0);
        let map_key = Word::from([
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ]);

        let link_map = LinkMap::new(map_ptr, process);

        let (set_op, entry_ptr) = link_map.compute_set_operation(LexicographicWord::from(map_key));

        Ok(vec![AdviceMutation::ExtendStack {
            values: vec![Felt::from(set_op as u8), Felt::from(entry_ptr)],
        }])
    }

    /// Handles a `LINK_MAP_GET_EVENT` emitted from a VM.
    ///
    /// Expected operand stack state before: [map_ptr, KEY]
    /// Advice stack state after: [get_operation, entry_ptr]
    pub fn handle_get_event(process: &ProcessState<'_>) -> Result<Vec<AdviceMutation>, EventError> {
        let map_ptr = process.get_stack_item(0);
        let map_key = Word::from([
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ]);

        let link_map = LinkMap::new(map_ptr, process);
        let (get_op, entry_ptr) = link_map.compute_get_operation(LexicographicWord::from(map_key));

        Ok(vec![AdviceMutation::ExtendStack {
            values: vec![Felt::from(get_op as u8), Felt::from(entry_ptr)],
        }])
    }

    /// Returns `true` if the map is empty, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.head().is_none()
    }

    /// Returns an iterator over the link map entries.
    pub fn iter(&self) -> impl Iterator<Item = Entry> {
        LinkMapIter {
            current_entry_ptr: self.head().unwrap_or(0),
            map: *self,
        }
    }

    // PRIVATE METHODS
    // --------------------------------------------------------------------------------------------

    /// Returns the entry pointer at the head of the map or `None` if the map is empty.
    fn head(&self) -> Option<u32> {
        // Returns None if the value was either not yet initialized or points to 0.
        // It can point to 0 for example if a get operation is executed before a set operation,
        // which initializes the value in memory to 0 but does not change it.
        self.get_kernel_mem_value(self.map_ptr).and_then(|head_ptr| {
            if head_ptr == ZERO {
                None
            } else {
                Some(u32::try_from(head_ptr).expect("head ptr should be a valid ptr"))
            }
        })
    }

    /// Returns the [`Entry`] at the given pointer.
    fn entry(&self, entry_ptr: u32) -> Entry {
        let key = self.key(entry_ptr);
        let (value0, value1) = self.value(entry_ptr);
        let metadata = self.metadata(entry_ptr);

        Entry {
            ptr: entry_ptr,
            metadata,
            key,
            value0,
            value1,
        }
    }

    /// Returns the key of the entry at the given pointer.
    fn key(&self, entry_ptr: u32) -> LexicographicWord {
        LexicographicWord::from(
            self.get_kernel_mem_word(entry_ptr + 4).expect("entry pointer should be valid"),
        )
    }

    /// Returns the values of the entry at the given pointer.
    fn value(&self, entry_ptr: u32) -> (Word, Word) {
        let value0 =
            self.get_kernel_mem_word(entry_ptr + 8).expect("entry pointer should be valid");
        let value1 =
            self.get_kernel_mem_word(entry_ptr + 12).expect("entry pointer should be valid");
        (value0, value1)
    }

    /// Returns the metadata of the entry at the given pointer.
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

    /// Computes what needs to be done to insert the given key into the link map.
    ///
    /// If the key already exists in the map, then its value must be updated and
    /// [`SetOperation::Update`] and the pointer to the existing entry are returned.
    ///
    /// If the key does not exist in the map, find the place where it has to be inserted. This can
    /// be at the head of the list ([`SetOperation::InsertAtHead`]) if the key is smaller than all
    /// existing keys or if the map is empty. Otherwise it is after an existing entry
    /// ([`SetOperation::InsertAfterEntry`]) in which case the key must be greater than the entry's
    /// key after which it is inserted and smaller than the entry before which it is inserted
    /// (unless it is the end of the map).
    fn compute_set_operation(&self, key: LexicographicWord) -> (SetOperation, u32) {
        let Some(current_head) = self.head() else {
            return (SetOperation::InsertAtHead, 0);
        };

        let mut last_entry_ptr: u32 = current_head;

        for entry in self.iter() {
            match key.cmp(&entry.key) {
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

    /// Computes a get operation for a key in a link map.
    ///
    /// If the key exists, then [`GetOperation::Found`] is returned and the pointer to it.
    ///
    /// If it does not exist, its absence must be proven, otherwise the host could lie. To do that,
    /// the in-kernel link map validates that the key is not in the list, so this function returns
    /// information pointing to the entry where the key would be if it existed.
    ///
    /// The way to compute this is the same as a set operation, so this function simply remaps its
    /// output.
    fn compute_get_operation(&self, key: LexicographicWord) -> (GetOperation, u32) {
        let (set_op, entry_ptr) = self.compute_set_operation(key);
        let get_op = match set_op {
            SetOperation::Update => GetOperation::Found,
            SetOperation::InsertAtHead => GetOperation::AbsentAtHead,
            SetOperation::InsertAfterEntry => GetOperation::AbsentAfterEntry,
        };
        (get_op, entry_ptr)
    }

    // HELPER METHODS
    // --------------------------------------------------------------------------------------------

    fn get_kernel_mem_value(&self, addr: u32) -> Option<Felt> {
        self.process.get_mem_value(ContextId::root(), addr)
    }

    fn get_kernel_mem_word(&self, addr: u32) -> Option<Word> {
        self.process
            .get_mem_word(ContextId::root(), addr)
            .expect("address should be word-aligned")
    }
}

// LINK MAP ITER
// ================================================================================================

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

// LINK MAP TYPES
// ================================================================================================

/// An entry in a [`LinkMap`].
///
/// Exposed for testing purposes only.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub ptr: u32,
    pub metadata: EntryMetadata,
    pub key: LexicographicWord,
    pub value0: Word,
    pub value1: Word,
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

// HELPER TYPES
// ================================================================================================

/// The operation needed to get a key or prove its absence.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum GetOperation {
    Found = 0,
    AbsentAtHead = 1,
    AbsentAfterEntry = 2,
}

/// The operation needed to set a key.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum SetOperation {
    Update = 0,
    InsertAtHead = 1,
    InsertAfterEntry = 2,
}
