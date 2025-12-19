use alloc::string::String;
use alloc::vec::Vec;

use miden_core::FieldElement;
use miden_objects::account::{AccountComponent, StorageMap, StorageSlot, StorageSlotName};
use miden_objects::assembly::Library;
use miden_objects::assembly::diagnostics::NamedSource;
use miden_objects::{Felt, Word};

use crate::transaction::TransactionKernel;

// CONSTANTS
// ================================================================================================

/// The maximum number of elements the array can store.
///
/// Since indices are represented as a single [`Felt`] element in the key `[index, 0, 0, 0]`,
/// the array can store up to 2^64 - 2^32 + 1 elements (indices 0 to 2^64 - 2^32).
///
/// [`Felt`]: miden_objects::Felt
pub const ARRAY_MAX_ELEMENTS: u64 = 0xffffffff00000001;

/// The MASM template for the Array component.
/// The placeholder `{{DATA_SLOT}}` is substituted with the actual slot name at construction time.
const ARRAY_MASM_TEMPLATE: &str = include_str!("../../asm/account_components/array.masm");

// ARRAY COMPONENT
// ================================================================================================

/// An [`AccountComponent`] providing an array data structure for storing words.
///
/// This component provides a sparse array backed by a StorageMap that can store up to
/// [`ARRAY_MAX_ELEMENTS`] elements. It supports `set` and `get` operations for storing and
/// retrieving words by index.
///
/// When linking against this component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib))
/// must be available to the assembler which is the case when using [`CodeBuilder`][builder].
///
/// The procedures of this component are:
/// - `set`: Stores a word at the specified index. Returns the old value.
/// - `get`: Retrieves a word at the specified index. Returns zero if not set.
///
/// This component supports all account types.
///
/// ## Capacity
///
/// The array can store up to [`ARRAY_MAX_ELEMENTS`] elements.
///
/// ## Storage Layout
///
/// - [`Self::data_slot`]: A StorageMap where key `[index, 0, 0, 0]` maps to the stored word
///
/// ## Configurable Storage Slot
///
/// The data slot can be configured at construction time, allowing multiple independent
/// arrays to coexist in the same account by using different slot names.
///
/// [builder]: crate::utils::CodeBuilder
pub struct Array {
    /// Initial elements to populate the array with, as (index, value) pairs.
    initial_elements: Vec<(Felt, Word)>,
    /// The storage slot name for the array data.
    data_slot: StorageSlotName,
}

impl Array {
    /// Creates a new [`Array`] component with the specified data slot and no initial elements.
    ///
    /// # Arguments
    /// * `data_slot` - The storage slot name where the array data will be stored.
    pub fn new(data_slot: StorageSlotName) -> Self {
        Self { initial_elements: Vec::new(), data_slot }
    }

    /// Creates a new [`Array`] component with the given initial elements.
    ///
    /// Elements are provided as (index, value) pairs. Any index not specified will
    /// return the zero word when accessed.
    ///
    /// # Arguments
    /// * `data_slot` - The storage slot name where the array data will be stored.
    /// * `elements` - Initial elements as (index, value) pairs.
    pub fn with_elements(
        data_slot: StorageSlotName,
        elements: impl IntoIterator<Item = (Felt, Word)>,
    ) -> Self {
        Self {
            initial_elements: elements.into_iter().collect(),
            data_slot,
        }
    }

    /// Creates a new [`Array`] component from a contiguous slice of words.
    ///
    /// The words are stored at indices 0, 1, 2, ... in order.
    ///
    /// # Arguments
    /// * `data_slot` - The storage slot name where the array data will be stored.
    /// * `elements` - Words to store at consecutive indices starting from 0.
    pub fn from_slice(data_slot: StorageSlotName, elements: &[Word]) -> Self {
        let initial_elements = elements
            .iter()
            .enumerate()
            .map(|(i, word)| (Felt::new(i as u64), *word))
            .collect();
        Self { initial_elements, data_slot }
    }

    /// Returns a reference to the [`StorageSlotName`] where the array data is stored.
    pub fn data_slot(&self) -> &StorageSlotName {
        &self.data_slot
    }

    /// Generates the MASM source code for this array component by substituting
    /// the data slot name into the template.
    fn generate_masm_source(&self) -> String {
        ARRAY_MASM_TEMPLATE.replace("{{DATA_SLOT}}", self.data_slot.as_str())
    }

    /// Generates the compiled [`Library`] for this array component with the given component name.
    ///
    /// The `component_name` determines the module path used to reference the array's procedures.
    /// For example, with `component_name = "myarray::data"`, the procedures would be called as
    /// `myarray::data::get` and `myarray::data::set`.
    ///
    /// This can be used to link the array component's procedures into transaction scripts
    /// or other code that needs to call the array's `get` and `set` procedures.
    pub fn generate_library(&self, component_name: &str) -> Library {
        let masm_source = self.generate_masm_source();
        let source = NamedSource::new(component_name, masm_source);
        TransactionKernel::assembler()
            .assemble_library([source])
            .expect("Array MASM template should be valid")
    }
}

impl From<Array> for AccountComponent {
    fn from(array: Array) -> Self {
        // Generate the MASM source with the configured slot name
        let masm_source = array.generate_masm_source();

        // Assemble the library dynamically
        let source = NamedSource::new("array::component", masm_source);
        let library = TransactionKernel::assembler()
            .assemble_library([source])
            .expect("Array MASM template should be valid");

        // Data slot: StorageMap with initial elements
        let map_entries = array
            .initial_elements
            .into_iter()
            .map(|(index, value)| (Word::from([index, Felt::ZERO, Felt::ZERO, Felt::ZERO]), value));

        let storage_slots = vec![StorageSlot::with_map(
            array.data_slot,
            StorageMap::with_entries(map_entries).unwrap(),
        )];

        AccountComponent::new(library, storage_slots)
            .expect("Array component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

#[cfg(test)]
mod tests {
    use miden_objects::Word;
    use miden_objects::account::AccountBuilder;

    use super::*;
    use crate::account::auth::NoAuth;

    #[test]
    fn test_array_creation_empty() {
        let slot =
            StorageSlotName::new("myproject::myarray::data").expect("slot name should be valid");
        let array = Array::new(slot);
        let component: AccountComponent = array.into();

        // Verify component was created successfully with one storage slot (data)
        assert_eq!(component.storage_slots().len(), 1);
    }

    #[test]
    fn test_array_creation_with_elements() {
        let elements = vec![
            (Felt::new(0), Word::from([1, 2, 3, 4u32])),
            (Felt::new(5), Word::from([5, 6, 7, 8u32])),
            (Felt::new(1000), Word::from([9, 10, 11, 12u32])),
        ];
        let slot =
            StorageSlotName::new("myproject::myarray::data").expect("slot name should be valid");
        let array = Array::with_elements(slot, elements);
        let component: AccountComponent = array.into();

        // Verify component was created successfully
        assert_eq!(component.storage_slots().len(), 1);
    }

    #[test]
    fn test_array_from_slice() {
        let elements = vec![
            Word::from([1, 2, 3, 4u32]),
            Word::from([5, 6, 7, 8u32]),
            Word::from([9, 10, 11, 12u32]),
        ];
        let slot =
            StorageSlotName::new("myproject::myarray::data").expect("slot name should be valid");
        let array = Array::from_slice(slot, &elements);
        let component: AccountComponent = array.into();

        // Verify component was created successfully
        assert_eq!(component.storage_slots().len(), 1);
    }

    #[test]
    fn test_array_account_integration() {
        let data_slot =
            StorageSlotName::new("myproject::myarray::data").expect("slot name should be valid");
        let elements = vec![
            (Felt::new(0), Word::from([1, 2, 3, 4u32])),
            (Felt::new(1), Word::from([5, 6, 7, 8u32])),
            (Felt::new(1000), Word::from([9, 10, 11, 12u32])),
        ];
        let array = Array::with_elements(data_slot.clone(), elements.clone());

        // Build an account with the Array component
        let account = AccountBuilder::new([0u8; 32])
            .with_auth_component(NoAuth)
            .with_component(array)
            .build()
            .expect("account building should succeed");

        // Verify data elements are stored correctly
        for (index, expected) in &elements {
            let key = Word::from([*index, Felt::ZERO, Felt::ZERO, Felt::ZERO]);
            let value = account
                .storage()
                .get_map_item(&data_slot, key)
                .expect("data slot should contain element");
            assert_eq!(&value, expected, "element at index {} should match", index);
        }
    }

    #[test]
    fn test_multiple_arrays_different_slots() {
        // Create two arrays with different slot names
        let slot1 =
            StorageSlotName::new("myproject::array1::data").expect("slot name should be valid");
        let slot2 =
            StorageSlotName::new("myproject::array2::data").expect("slot name should be valid");

        let array1 =
            Array::with_elements(slot1.clone(), [(Felt::new(0), Word::from([1, 1, 1, 1u32]))]);
        let array2 =
            Array::with_elements(slot2.clone(), [(Felt::new(0), Word::from([2, 2, 2, 2u32]))]);

        // Build an account with both Array components
        let account = AccountBuilder::new([0u8; 32])
            .with_auth_component(NoAuth)
            .with_component(array1)
            .with_component(array2)
            .build()
            .expect("account building should succeed");

        // Verify both arrays have their data stored correctly
        let key = Word::from([0u32, 0, 0, 0]);

        let value1 = account
            .storage()
            .get_map_item(&slot1, key)
            .expect("slot1 should contain element");
        assert_eq!(value1, Word::from([1, 1, 1, 1u32]));

        let value2 = account
            .storage()
            .get_map_item(&slot2, key)
            .expect("slot2 should contain element");
        assert_eq!(value2, Word::from([2, 2, 2, 2u32]));
    }
}
