use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use core::error::Error;
use core::fmt::{self, Display};

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_core::{Felt, Word};
use miden_crypto::dsa::rpo_falcon512::{self};
use miden_processor::DeserializationError;
use thiserror::Error;

use crate::asset::TokenSymbol;
use crate::utils::sync::LazyLock;

/// A global registry for template converters.
///
/// It is used during component instantiation to dynamically convert template placeholders into
/// their respective storage items.
pub static TEMPLATE_REGISTRY: LazyLock<TemplateRegistry> = LazyLock::new(|| {
    let mut registry = TemplateRegistry::new();
    registry.register_felt_type::<u8>();
    registry.register_felt_type::<u16>();
    registry.register_felt_type::<u32>();
    registry.register_felt_type::<Felt>();
    registry.register_felt_type::<TokenSymbol>();
    registry.register_word_type::<Word>();
    registry.register_word_type::<rpo_falcon512::PublicKey>();
    registry
});

// STORAGE VALUE NAME
// ================================================================================================

/// A simple wrapper type around a string key that identifies values.
///
/// A storage value name is a string that identifies dynamic values within a component's metadata
/// storage entries.
///
/// These names can be chained together, in a way that allows composing unique keys for inner
/// templated elements.
///
/// At component instantiation, a map of names to values must be provided to dynamically
/// replace these placeholders with the instance's actual values.
#[derive(Clone, Debug, Ord, PartialOrd, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(::serde::Deserialize, ::serde::Serialize))]
#[cfg_attr(feature = "std", serde(transparent))]
pub struct StorageValueName {
    fully_qualified_name: String,
}

impl StorageValueName {
    /// Creates a new [`StorageValueName`] from the provided string.
    ///
    /// A [`StorageValueName`] serves as an identifier for storage items that are determined at
    /// instantiation time of an [AccountComponentTemplate](super::super::AccountComponentTemplate).
    ///
    /// The key can consist of one or more segments separated by dots (`.`).
    /// Each segment must be non-empty and may contain only alphanumeric characters, underscores
    /// (`_`), or hyphens (`-`).
    ///
    /// # Errors
    ///
    /// This method returns an error if:
    /// - Any segment (or the whole key) is empty.
    /// - Any segment contains invalid characters.
    pub fn new(base: impl Into<String>) -> Result<Self, StorageValueNameError> {
        let base: String = base.into();
        for segment in base.split('.') {
            Self::validate_segment(segment)?;
        }
        Ok(Self { fully_qualified_name: base })
    }

    /// Creates an empty [`StorageValueName`].
    pub(crate) fn empty() -> Self {
        StorageValueName { fully_qualified_name: String::default() }
    }

    /// Adds a suffix to the storage value name, separated by a period.
    #[must_use]
    pub fn with_suffix(self, suffix: &StorageValueName) -> StorageValueName {
        let mut key = self;
        if !suffix.as_str().is_empty() {
            if !key.as_str().is_empty() {
                key.fully_qualified_name.push('.');
            }
            key.fully_qualified_name.push_str(suffix.as_str());
        }

        key
    }

    /// Returns the fully qualified name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.fully_qualified_name
    }

    fn validate_segment(segment: &str) -> Result<(), StorageValueNameError> {
        if segment.is_empty() {
            return Err(StorageValueNameError::EmptySegment);
        }
        if let Some(offending_char) =
            segment.chars().find(|&c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        {
            return Err(StorageValueNameError::InvalidCharacter {
                part: segment.to_string(),
                character: offending_char,
            });
        }

        Ok(())
    }
}

impl Display for StorageValueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serializable for StorageValueName {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(&self.fully_qualified_name);
    }
}

impl Deserializable for StorageValueName {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let key: String = source.read()?;
        Ok(StorageValueName { fully_qualified_name: key })
    }
}

#[derive(Debug, Error)]
pub enum StorageValueNameError {
    #[error("key segment is empty")]
    EmptySegment,
    #[error("key segment '{part}' contains invalid character '{character}'")]
    InvalidCharacter { part: String, character: char },
}

// TEMPLATE TYPE ERROR
// ================================================================================================

/// Errors that can occur when parsing or converting template types.
///
/// This enum covers various failure cases including parsing errors, conversion errors,
/// unsupported conversions, and cases where a required type is not found in the registry.
#[derive(Debug, Error)]
pub enum TemplateTypeError {
    #[error("conversion error: {0}")]
    ConversionError(String),
    #[error("felt type ` {0}` not found in the type registry")]
    FeltTypeNotFound(TemplateType),
    #[error("invalid type name `{0}`: {1}")]
    InvalidTypeName(String, String),
    #[error("failed to parse input `{input}` as `{template_type}`")]
    ParseError {
        input: String,
        template_type: TemplateType,
        source: Box<dyn Error + Send + Sync + 'static>,
    },
    #[error("word type ` {0}` not found in the type registry")]
    WordTypeNotFound(TemplateType),
}

impl TemplateTypeError {
    /// Creates a [`TemplateTypeError::ParseError`].
    pub fn parse(
        input: impl Into<String>,
        template_type: TemplateType,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        TemplateTypeError::ParseError {
            input: input.into(),
            template_type,
            source: Box::new(source),
        }
    }
}

// TEMPLATE TYPE
// ================================================================================================

/// A newtype wrapper around a `String`, representing a template's type identifier.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[cfg_attr(feature = "std", derive(::serde::Deserialize, ::serde::Serialize))]
#[cfg_attr(feature = "std", serde(transparent))]
pub struct TemplateType(String);

impl TemplateType {
    /// Creates a new [`TemplateType`] from a `String`.
    ///
    /// The name must follow a Rust-style namespace format, consisting of one or more segments
    /// (non-empty, and alphanumerical) separated by double-colon (`::`) delimiters.
    ///
    /// # Errors
    ///
    /// - If the identifier is empty.
    /// - If it is composed of one or more segments separated by `::`.
    /// - If any segment is empty or contains something other than alphanumerical
    ///   characters/underscores.
    pub fn new(s: impl Into<String>) -> Result<Self, TemplateTypeError> {
        let s = s.into();
        if s.is_empty() {
            return Err(TemplateTypeError::InvalidTypeName(
                s.clone(),
                "template type identifier is empty".to_string(),
            ));
        }
        for segment in s.split("::") {
            if segment.is_empty() {
                return Err(TemplateTypeError::InvalidTypeName(
                    s.clone(),
                    "empty segment in template type identifier".to_string(),
                ));
            }
            if !segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(TemplateTypeError::InvalidTypeName(
                    s.clone(),
                    format!("segment '{segment}' contains invalid characters"),
                ));
            }
        }
        Ok(Self(s))
    }

    /// Returns the [`TemplateType`] for the native [`Felt`] type.
    pub fn native_felt() -> TemplateType {
        TemplateType::new("felt").expect("type is well formed")
    }

    /// Returns the [`TemplateType`] for the native [`Word`] type.
    pub fn native_word() -> TemplateType {
        TemplateType::new("word").expect("type is well formed")
    }

    /// Returns the [`TemplateType`] for storage map placeholders.
    pub fn storage_map() -> TemplateType {
        TemplateType::new("map").expect("type is well formed")
    }

    /// Returns a reference to the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TemplateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serializable for TemplateType {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(self.0.clone())
    }
}

impl Deserializable for TemplateType {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let id: String = source.read()?;

        TemplateType::new(id).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TEMPLATE REQUIREMENT
// ================================================================================================

/// Describes the expected type and additional metadata for a templated storage entry.
///
/// A `PlaceholderTypeRequirement` specifies the expected type identifier for a storage entry as
/// well as an optional description. This information is used to validate and provide context for
/// dynamic storage items.
#[derive(Debug)]
pub struct PlaceholderTypeRequirement {
    /// The expected type identifier.
    pub r#type: TemplateType,
    /// An optional description providing additional context.
    pub description: Option<String>,
}

// TEMPLATE TRAITS
// ================================================================================================

/// Trait for converting a string into a single `Felt`.
pub trait TemplateFelt {
    /// Returns the type identifier.
    fn type_name() -> TemplateType;
    /// Parses the input string into a `Felt`.
    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError>;
}

/// Trait for converting a string into a single `Word`.
pub trait TemplateWord {
    /// Returns the type identifier.
    fn type_name() -> TemplateType;
    /// Parses the input string into a `Word`.
    fn parse_word(input: &str) -> Result<Word, TemplateTypeError>;
}

// FELT IMPLS FOR NATIVE TYPES
// ================================================================================================

impl TemplateFelt for u8 {
    fn type_name() -> TemplateType {
        TemplateType::new("u8").expect("type is well formed")
    }

    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError> {
        let native: u8 = input
            .parse()
            .map_err(|err| TemplateTypeError::parse(input.to_string(), Self::type_name(), err))?;
        Ok(Felt::from(native))
    }
}

impl TemplateFelt for u16 {
    fn type_name() -> TemplateType {
        TemplateType::new("u16").expect("type is well formed")
    }

    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError> {
        let native: u16 = input
            .parse()
            .map_err(|err| TemplateTypeError::parse(input.to_string(), Self::type_name(), err))?;
        Ok(Felt::from(native))
    }
}

impl TemplateFelt for u32 {
    fn type_name() -> TemplateType {
        TemplateType::new("u32").expect("type is well formed")
    }

    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError> {
        let native: u32 = input
            .parse()
            .map_err(|err| TemplateTypeError::parse(input.to_string(), Self::type_name(), err))?;
        Ok(Felt::from(native))
    }
}

impl TemplateFelt for Felt {
    fn type_name() -> TemplateType {
        TemplateType::new("felt").expect("type is well formed")
    }

    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError> {
        let n = if let Some(hex) = input.strip_prefix("0x").or_else(|| input.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16)
        } else {
            input.parse::<u64>()
        }
        .map_err(|err| TemplateTypeError::parse(input.to_string(), Self::type_name(), err))?;
        Felt::try_from(n).map_err(|_| TemplateTypeError::ConversionError(input.to_string()))
    }
}

impl TemplateFelt for TokenSymbol {
    fn type_name() -> TemplateType {
        TemplateType::new("token_symbol").expect("type is well formed")
    }
    fn parse_felt(input: &str) -> Result<Felt, TemplateTypeError> {
        let token = TokenSymbol::new(input)
            .map_err(|err| TemplateTypeError::parse(input.to_string(), Self::type_name(), err))?;
        Ok(Felt::from(token))
    }
}

// WORD IMPLS FOR NATIVE TYPES
// ================================================================================================

#[derive(Debug, Error)]
#[error("error parsing word: {0}")]
struct WordParseError(String);

/// Pads a hex string to 64 characters (excluding the 0x prefix).
///
/// If the input starts with "0x" and has fewer than 64 hex characters after the prefix,
/// it will be left-padded with zeros. Otherwise, returns the input unchanged.
fn pad_hex_string(input: &str) -> String {
    if input.starts_with("0x") && input.len() < 66 {
        // 66 = "0x" + 64 hex chars
        let hex_part = &input[2..];
        let padding = "0".repeat(64 - hex_part.len());
        format!("0x{}{}", padding, hex_part)
    } else {
        input.to_string()
    }
}

impl TemplateWord for Word {
    fn type_name() -> TemplateType {
        TemplateType::native_word()
    }
    fn parse_word(input: &str) -> Result<Word, TemplateTypeError> {
        let padded_input = pad_hex_string(input);

        Word::try_from(padded_input.as_str()).map_err(|err| {
            TemplateTypeError::parse(
                input.to_string(), // Use original input in error
                Self::type_name(),
                WordParseError(err.to_string()),
            )
        })
    }
}

impl TemplateWord for rpo_falcon512::PublicKey {
    fn type_name() -> TemplateType {
        TemplateType::new("auth::rpo_falcon512::pub_key").expect("type is well formed")
    }
    fn parse_word(input: &str) -> Result<Word, TemplateTypeError> {
        let padded_input = pad_hex_string(input);

        Word::try_from(padded_input.as_str()).map_err(|err| {
            TemplateTypeError::parse(
                input.to_string(), // Use original input in error
                Self::type_name(),
                WordParseError(err.to_string()),
            )
        })
    }
}

// TYPE ALIASES FOR CONVERTER CLOSURES
// ================================================================================================

/// Type alias for a function that converts a string into a [`Felt`] value.
type TemplateFeltConverter = fn(&str) -> Result<Felt, TemplateTypeError>;

/// Type alias for a function that converts a string into a [`Word`].
type TemplateWordConverter = fn(&str) -> Result<Word, TemplateTypeError>;

// TODO: Implement converting to list of words for multi-slot values

// TEMPLATE REGISTRY
// ================================================================================================

/// Registry for template converters.
///
/// This registry maintains mappings from type identifiers (as strings) to conversion functions for
/// [`Felt`], [`Word`], and [`Vec<Word>`] types. It is used to dynamically parse template inputs
/// into their corresponding storage representations.
#[derive(Clone, Debug, Default)]
pub struct TemplateRegistry {
    felt: BTreeMap<TemplateType, TemplateFeltConverter>,
    word: BTreeMap<TemplateType, TemplateWordConverter>,
}

impl TemplateRegistry {
    /// Creates a new, empty `TemplateRegistry`.
    ///
    /// The registry is initially empty and conversion functions can be registered using the
    /// `register_*_type` methods.
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    /// Registers a `TemplateFelt` converter, to interpret a string as a [`Felt``].
    pub fn register_felt_type<T: TemplateFelt + 'static>(&mut self) {
        let key = T::type_name();
        self.felt.insert(key, T::parse_felt);
    }

    /// Registers a `TemplateWord` converter, to interpret a string as a [`Word`].
    pub fn register_word_type<T: TemplateWord + 'static>(&mut self) {
        let key = T::type_name();
        self.word.insert(key, T::parse_word);
    }

    /// Attempts to parse a string into a `Felt` using the registered converter for the given type
    /// name.
    ///
    /// # Arguments
    ///
    /// - type_name: A string that acts as the type identifier.
    /// - value: The string representation of the value to be parsed.
    ///
    /// # Errors
    ///
    /// - If the type is not registered or if the conversion fails.
    pub fn try_parse_felt(
        &self,
        type_name: &TemplateType,
        value: &str,
    ) -> Result<Felt, TemplateTypeError> {
        let converter = self
            .felt
            .get(type_name)
            .ok_or(TemplateTypeError::FeltTypeNotFound(type_name.clone()))?;
        converter(value)
    }

    /// Attempts to parse a string into a `Word` using the registered converter for the given type
    /// name.
    ///
    /// # Arguments
    ///
    /// - type_name: A string that acts as the type identifier.
    /// - value: The string representation of the value to be parsed.
    ///
    /// # Errors
    ///
    /// - If the type is not registered or if the conversion fails.
    pub fn try_parse_word(
        &self,
        type_name: &TemplateType,
        value: &str,
    ) -> Result<Word, TemplateTypeError> {
        let converter = self
            .word
            .get(type_name)
            .ok_or(TemplateTypeError::WordTypeNotFound(type_name.clone()))?;
        converter(value)
    }

    /// Returns `true` if a `TemplateFelt` is registered for the given type.
    pub fn contains_felt_type(&self, type_name: &TemplateType) -> bool {
        self.felt.contains_key(type_name)
    }

    /// Returns `true` if a `TemplateWord` is registered for the given type.
    pub fn contains_word_type(&self, type_name: &TemplateType) -> bool {
        self.word.contains_key(type_name)
    }
}
