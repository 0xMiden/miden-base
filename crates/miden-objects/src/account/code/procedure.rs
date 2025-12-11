use alloc::string::ToString;
use alloc::sync::Arc;

use miden_core::mast::MastForest;
use miden_core::prettier::PrettyPrint;
use miden_processor::{MastNode, MastNodeExt, MastNodeId};

use super::Felt;
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{AccountError, Word};

// ACCOUNT PROCEDURE INFO
// ================================================================================================

/// Information about a procedure exposed in a public account interface.
///
/// The info included the MAST root of the procedure, the storage offset applied to all account
/// storage-related accesses made by this procedure and the storage size allowed to be accessed
/// by this procedure.
///
/// The offset is applied to any accesses made from within the procedure to the associated
/// account's storage. For example, if storage offset for a procedure is set to 1, a call
/// to the account::get_item(storage_slot=4) made from this procedure would actually access
/// storage slot with index 5.
///
/// The size is used to limit how many storage slots a given procedure can access in the associated
/// account's storage. For example, if storage size for a procedure is set to 3, the procedure will
/// be bounded to access storage slots in the range [storage_offset, storage_offset + 3 - 1].
/// Furthermore storage_size = 0 indicates that a procedure does not need to access storage.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct AccountProcedureInfo {
    mast_root: Word,
    storage_offset: u8,
    storage_size: u8,
}

impl AccountProcedureInfo {
    /// The number of field elements that represent an [`AccountProcedureInfo`] in kernel memory.
    pub const NUM_ELEMENTS: usize = 4;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of an [AccountProcedureInfo].
    ///
    /// # Errors
    /// - If `storage_size` is 0 and `storage_offset` is not 0.
    /// - If `storage_size + storage_offset` is greater than `MAX_NUM_STORAGE_SLOTS`.
    pub fn new(
        mast_root: Word,
        storage_offset: u8,
        storage_size: u8,
    ) -> Result<Self, AccountError> {
        if storage_size == 0 && storage_offset != 0 {
            return Err(AccountError::PureProcedureWithStorageOffset);
        }

        // Check if the addition would exceed AccountStorage::MAX_NUM_STORAGE_SLOTS (= 255) which is
        // the case if the addition overflows.
        if storage_offset.checked_add(storage_size).is_none() {
            return Err(AccountError::StorageOffsetPlusSizeOutOfBounds(
                storage_offset as u16 + storage_size as u16,
            ));
        }

        Ok(Self { mast_root, storage_offset, storage_size })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the procedure's mast root.
    pub fn mast_root(&self) -> &Word {
        &self.mast_root
    }

    /// Returns the procedure root as a slice of field elements.
    pub fn as_elements(&self) -> &[Felt] {
        self.mast_root.as_elements()
    }

    /// Returns the procedure's storage offset.
    pub fn storage_offset(&self) -> u8 {
        self.storage_offset
    }

    /// Returns the procedure's storage size.
    pub fn storage_size(&self) -> u8 {
        self.storage_size
    }
}

impl From<AccountProcedureInfo> for Word {
    fn from(root: AccountProcedureInfo) -> Self {
        *root.mast_root()
    }
}

impl Serializable for AccountProcedureInfo {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(self.mast_root);
        target.write_u8(self.storage_offset);
        target.write_u8(self.storage_size)
    }

    fn get_size_hint(&self) -> usize {
        self.mast_root.get_size_hint()
            + self.storage_offset.get_size_hint()
            + self.storage_size.get_size_hint()
    }
}

impl Deserializable for AccountProcedureInfo {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let mast_root: Word = source.read()?;
        let storage_offset = source.read_u8()?;
        let storage_size = source.read_u8()?;
        Self::new(mast_root, storage_offset, storage_size)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// PRINTABLE PROCEDURE
// ================================================================================================

/// A printable representation of a single account procedure.
#[derive(Debug, Clone)]
pub struct PrintableProcedure {
    mast: Arc<MastForest>,
    procedure_info: AccountProcedureInfo,
    entrypoint: MastNodeId,
}

impl PrintableProcedure {
    /// Creates a new PrintableProcedure instance from its components.
    pub(crate) fn new(
        mast: Arc<MastForest>,
        procedure_info: AccountProcedureInfo,
        entrypoint: MastNodeId,
    ) -> Self {
        Self { mast, procedure_info, entrypoint }
    }

    fn entrypoint(&self) -> &MastNode {
        &self.mast[self.entrypoint]
    }

    pub(crate) fn storage_offset(&self) -> u8 {
        self.procedure_info.storage_offset()
    }

    pub(crate) fn storage_size(&self) -> u8 {
        self.procedure_info.storage_size()
    }

    pub(crate) fn mast_root(&self) -> &Word {
        self.procedure_info.mast_root()
    }
}

impl PrettyPrint for PrintableProcedure {
    fn render(&self) -> miden_core::prettier::Document {
        use miden_core::prettier::*;

        indent(
            4,
            const_text("begin") + nl() + self.entrypoint().to_pretty_print(&self.mast).render(),
        ) + nl()
            + const_text("end")
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use miden_crypto::utils::{Deserializable, Serializable};

    use crate::account::{AccountCode, AccountProcedureInfo};

    #[test]
    fn test_serde_account_procedure() {
        let account_code = AccountCode::mock();

        let serialized = account_code.procedures()[0].to_bytes();
        let deserialized = AccountProcedureInfo::read_from_bytes(&serialized).unwrap();

        assert_eq!(account_code.procedures()[0], deserialized);
    }
}
