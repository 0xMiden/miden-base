//! Procedural macros for the Miden project.
//!
//! Provides derive macros and other procedural macros to reduce boilerplate
//! and ensure consistency across the Miden codebase.
//!
//! ## Available Macros
//!
//! ### `WordWrapper`
//!
//! A derive macro for tuple structs wrapping a `Word` type. Automatically generates
//! accessor methods and `From` trait implementations.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type, parse_macro_input};

/// Generates accessor methods for tuple structs wrapping a `Word` type.
///
/// Automatically implements:
/// - `new_unchecked(Word) -> Self` - Construct without further checks
/// - `as_elements(&self) -> &[Felt]` - Returns the elements representation
/// - `as_bytes(&self) -> [u8; 32]` - Returns the byte representation
/// - `to_hex(&self) -> String` - Returns a big-endian, hex-encoded string
/// - `as_word(&self) -> Word` - Returns the underlying Word
///
/// Note: This macro does NOT generate `From` trait implementations. If you need conversions
/// to/from `Word` or `[u8; 32]`, implement them manually for your type.
///
/// # Example
///
/// ```ignore
/// use miden_protocol_macros::WordWrapper;
/// use miden_crypto::word::Word;
///
/// #[derive(WordWrapper)]
/// pub struct NoteId(Word);
/// ```
///
/// This will generate implementations equivalent to:
///
/// ```ignore
/// impl NoteId {
///     /// Construct without further checks from a given `Word`
///     ///
///     /// # Warning
///     ///
///     /// This requires the caller to uphold the guarantees/invariants of this type (if any).
///     /// Check the type-level documentation for guarantees/invariants.
///     pub fn new_unchecked(word: Word) -> Self {
///         Self(word)
///     }
///
///     pub fn as_elements(&self) -> &[Felt] {
///         self.0.as_elements()
///     }
///
///     pub fn as_bytes(&self) -> [u8; 32] {
///         self.0.as_bytes()
///     }
///
///     pub fn to_hex(&self) -> String {
///         self.0.to_hex()
///     }
///
///     pub fn as_word(&self) -> Word {
///         self.0
///     }
/// }
/// ```
#[proc_macro_derive(WordWrapper)]
pub fn word_wrapper_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Validate that this is a tuple struct with a single field
    let field_type = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => match fields.unnamed.first() {
                Some(field) => &field.ty,
                None => {
                    return syn::Error::new_spanned(
                        &input,
                        "WordWrapper requires exactly one field",
                    )
                    .to_compile_error()
                    .into();
                },
            },
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "WordWrapper can only be derived for tuple structs with exactly one field",
                )
                .to_compile_error()
                .into();
            },
        },
        _ => {
            return syn::Error::new_spanned(&input, "WordWrapper can only be derived for structs")
                .to_compile_error()
                .into();
        },
    };

    // Verify that the field type is 'Word' (or a path ending in 'Word')
    if let Type::Path(type_path) = field_type {
        let last_segment = type_path.path.segments.last();
        if let Some(segment) = last_segment {
            if segment.ident != "Word" {
                return syn::Error::new_spanned(
                    field_type,
                    "WordWrapper can only be derived for types wrapping a 'Word' field",
                )
                .to_compile_error()
                .into();
            }
        } else {
            return syn::Error::new_spanned(
                field_type,
                "WordWrapper can only be derived for types wrapping a 'Word' field",
            )
            .to_compile_error()
            .into();
        }
    } else {
        return syn::Error::new_spanned(
            field_type,
            "WordWrapper can only be derived for types wrapping a 'Word' field",
        )
        .to_compile_error()
        .into();
    }

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Construct without further checks from a given `Word`
            ///
            /// # Warning
            ///
            /// This requires the caller to uphold the guarantees/invariants of this type (if any).
            /// Check the type-level documentation for guarantees/invariants.
            pub fn new_unchecked(word: Word) -> Self {
                Self(word)
            }

            /// Returns the elements representation of this value.
            pub fn as_elements(&self) -> &[Felt] {
                self.0.as_elements()
            }

            /// Returns the byte representation of this value.
            pub fn as_bytes(&self) -> [u8; 32] {
                self.0.as_bytes()
            }

            /// Returns a big-endian, hex-encoded string.
            pub fn to_hex(&self) -> String {
                self.0.to_hex()
            }

            /// Returns the underlying word of this value.
            pub fn as_word(&self) -> Word {
                self.0
            }
        }
    };

    TokenStream::from(expanded)
}
