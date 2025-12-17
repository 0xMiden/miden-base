#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod auth;
pub use auth::AuthScheme;

pub mod account;
pub mod errors;
pub mod note;
pub mod utils;

#[cfg(any(feature = "testing", test))]
pub mod testing;
