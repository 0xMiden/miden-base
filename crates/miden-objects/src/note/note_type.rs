use core::{fmt::Display, str::FromStr};

use crate::{
    Felt, NoteError,
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
};

// CONSTANTS
// ================================================================================================

// Keep these masks in sync with `miden-lib/asm/miden/kernels/tx/tx.masm`
const PUBLIC: u8 = 0b01;
const PRIVATE: u8 = 0b10;
const ENCRYPTED: u8 = 0b11;

// NOTE TYPE
// ================================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NoteType {
    /// Notes with this type have only their hash published to the network.
    Private = PRIVATE,

    /// Notes with this type are shared with the network encrypted.
    Encrypted = ENCRYPTED,

    /// Notes with this type are fully shared with the network.
    Public = PUBLIC,
}

// CONVERSIONS FROM NOTE TYPE
// ================================================================================================

impl From<NoteType> for Felt {
    fn from(id: NoteType) -> Self {
        Felt::new(id as u64)
    }
}

// CONVERSIONS INTO NOTE TYPE
// ================================================================================================

impl TryFrom<u8> for NoteType {
    type Error = NoteError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            PRIVATE => Ok(NoteType::Private),
            ENCRYPTED => Ok(NoteType::Encrypted),
            PUBLIC => Ok(NoteType::Public),
            _ => Err(NoteError::InvalidNoteType(value.into())),
        }
    }
}

impl TryFrom<u16> for NoteType {
    type Error = NoteError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u32> for NoteType {
    type Error = NoteError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_from(value as u64)
    }
}

impl TryFrom<u64> for NoteType {
    type Error = NoteError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let value: u8 = value.try_into().map_err(|_| NoteError::InvalidNoteType(value))?;
        value.try_into()
    }
}

impl TryFrom<Felt> for NoteType {
    type Error = NoteError;

    fn try_from(value: Felt) -> Result<Self, Self::Error> {
        value.as_int().try_into()
    }
}

impl FromStr for NoteType {
    type Err = NoteError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "private" => Ok(NoteType::Private),
            "encrypted" => Ok(NoteType::Encrypted),
            "public" => Ok(NoteType::Public),
            _ => Err(NoteError::InvalidNoteType(4)),
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteType {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        (*self as u8).write_into(target)
    }
}

impl Deserializable for NoteType {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let discriminat = u8::read_from(source)?;

        let note_type = match discriminat {
            PRIVATE => NoteType::Private,
            ENCRYPTED => NoteType::Encrypted,
            PUBLIC => NoteType::Public,
            v => {
                return Err(DeserializationError::InvalidValue(format!(
                    "value {} is not a valid NoteType",
                    v
                )));
            },
        };

        Ok(note_type)
    }
}

// DISPLAY
// ================================================================================================

impl Display for NoteType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NoteType::Private => write!(f, "Private"),
            NoteType::Encrypted => write!(f, "Encrypted"),
            NoteType::Public => write!(f, "Public"),
        }
    }
}

#[test]
fn test_from_str_note_type() {
    use crate::alloc::string::ToString;

    let private_type = NoteType::from_str("private").unwrap();
    assert_eq!(private_type, NoteType::Private);

    let public_type = NoteType::from_str("puBlIc").unwrap();
    assert_eq!(public_type, NoteType::Public);

    let encrypted_type = NoteType::from_str("eNcrYptEd").unwrap();
    assert_eq!(encrypted_type, NoteType::Encrypted);

    let invalid_type = NoteType::from_str("invalid").unwrap_err();
    assert_eq!(
        invalid_type.to_string(),
        alloc::string::String::from(
            "note type 100 does not match any of the valid note types 1, 2 or 3"
        )
    );
}
