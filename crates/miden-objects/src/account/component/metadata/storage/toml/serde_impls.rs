use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_core::Felt;
use serde::de::Error as DeError;
use serde::ser::{Error as SerError, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::type_registry::SCHEMA_TYPE_REGISTRY;
use super::super::{FeltSchema, SchemaTypeIdentifier, StorageValueName};
use crate::asset::TokenSymbol;

// FELT SCHEMA SERIALIZATION
// ================================================================================================

impl Serialize for FeltSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.felt_type() == SchemaTypeIdentifier::void() {
            let mut state = serializer.serialize_struct("FeltSchema", 2)?;
            state.serialize_field("type", &SchemaTypeIdentifier::void())?;
            if let Some(description) = self.description() {
                state.serialize_field("description", description)?;
            }
            return state.end();
        }

        let name = self.name().ok_or_else(|| {
            SerError::custom("invalid FeltSchema: non-void elements must have a name")
        })?;

        let mut state = serializer.serialize_struct("FeltSchema", 4)?;
        state.serialize_field("name", name)?;
        if let Some(description) = self.description() {
            state.serialize_field("description", description)?;
        }
        if self.felt_type() != SchemaTypeIdentifier::native_felt() {
            state.serialize_field("type", &self.felt_type())?;
        }
        if let Some(default_value) = self.default_value() {
            state.serialize_field(
                "default-value",
                &render_felt_default_value(&self.felt_type(), default_value)
                    .map_err(|err| SerError::custom(err.to_string()))?,
            )?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for FeltSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct RawFeltSchema {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, rename = "default-value")]
            default_value: Option<toml::Value>,
            #[serde(default, rename = "type")]
            r#type: Option<SchemaTypeIdentifier>,
            #[serde(flatten)]
            extra: BTreeMap<String, toml::Value>,
        }

        let raw = RawFeltSchema::deserialize(deserializer)?;

        let felt_type = raw.r#type.unwrap_or_else(SchemaTypeIdentifier::native_felt);
        if !raw.extra.is_empty() {
            let mut keys = raw.extra.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            return Err(D::Error::custom(format!(
                "felt schema contains unknown field(s): {}",
                keys.join(", ")
            )));
        }

        let description = raw.description.and_then(|description| {
            if description.trim().is_empty() {
                None
            } else {
                Some(description)
            }
        });

        if felt_type == SchemaTypeIdentifier::void() {
            if raw.name.is_some() {
                return Err(D::Error::custom("`type = \"void\"` elements must omit `name`"));
            }
            if raw.default_value.is_some() {
                return Err(D::Error::custom(
                    "`type = \"void\"` elements cannot define `default-value`",
                ));
            }

            let schema = FeltSchema::new_void();
            return Ok(match description {
                Some(description) => schema.with_description(description),
                None => schema,
            });
        }

        let Some(name) = raw.name else {
            return Err(D::Error::custom("non-void elements must define `name`"));
        };

        let name: StorageValueName =
            name.parse().map_err(|err| D::Error::custom(format!("invalid `name`: {err}")))?;

        let default_value = raw
            .default_value
            .map(toml_value_to_string::<D::Error>)
            .transpose()?
            .map(|default_value| {
                SCHEMA_TYPE_REGISTRY.try_parse_felt(&felt_type, &default_value).map_err(|err| {
                    D::Error::custom(format!(
                        "failed to parse {felt_type} as Felt for `default-value`: {err}"
                    ))
                })
            })
            .transpose()?;

        let schema = match default_value {
            Some(default_value) => {
                FeltSchema::new_typed_with_default(felt_type, name, default_value)
            },
            None => FeltSchema::new_typed(felt_type, name),
        };
        Ok(match description {
            Some(description) => schema.with_description(description),
            None => schema,
        })
    }
}

fn render_felt_default_value(
    schema_type: &SchemaTypeIdentifier,
    felt: Felt,
) -> Result<String, crate::account::component::SchemaTypeError> {
    match schema_type.as_str() {
        "void" => Ok("0".into()),
        "u8" | "u16" | "u32" => Ok(felt.as_int().to_string()),
        "token_symbol" => {
            let token = TokenSymbol::try_from(felt).map_err(|err| {
                crate::account::component::SchemaTypeError::ConversionError(err.to_string())
            })?;
            token.to_string().map_err(|err| {
                crate::account::component::SchemaTypeError::ConversionError(err.to_string())
            })
        },
        _ => Ok(format!("0x{:x}", felt.as_int())),
    }
}

fn toml_value_to_string<E: DeError>(value: toml::Value) -> Result<String, E> {
    match value {
        toml::Value::String(s) => Ok(s),
        toml::Value::Integer(i) => Ok(i.to_string()),
        toml::Value::Float(f) => Ok(f.to_string()),
        other => Err(E::custom(format!(
            "expected a scalar TOML value for `default-value`, got {other}"
        ))),
    }
}
