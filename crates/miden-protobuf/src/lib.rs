//! Generated structural Protobuf decoding and opt-in domain construction.
#![no_std]
extern crate alloc;
#[cfg(feature = "build")]
extern crate std;
#[cfg(feature = "build")]
pub mod build;
mod decode;
mod error;
mod message;
pub use decode::{
    DecodeField,
    DecodeRepeated,
    OptionalField,
    RepeatedField,
    RequiredField,
    ValueField,
    decode,
};
pub use error::{ConversionError, ConversionResultExt};
pub use message::{BuildUnchecked, DecodeMessage, Decoded, Verify, VerifyWith};
#[cfg(feature = "derive")]
pub use miden_protobuf_derive::ProtoDecodeFields;
pub use prost;
#[doc(hidden)]
pub mod __private {
    pub use alloc::vec::Vec;

    pub use crate::{
        ConversionError,
        DecodeMessage,
        OptionalField,
        RepeatedField,
        RequiredField,
        ValueField,
        decode,
    };
}
