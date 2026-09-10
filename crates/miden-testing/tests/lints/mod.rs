//! Source-level checks over the MASM sources.
//!
//! These read the `.masm` files rather than executing them, so they catch a convention that was
//! not followed - not a rule that does not hold. What the code actually does is covered by the
//! execution tests in the other modules.

mod note_consumers;
