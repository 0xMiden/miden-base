use alloc::string::String;

use serde::de::Error as _;
use serde::ser::{Error as SerError, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::type_registry::SCHEMA_TYPE_REGISTRY;
use super::super::{FeltSchema, SchemaTypeId, StorageValueName};

// FELT SCHEMA SERIALIZATION
// ================================================================================================

impl Serialize for FeltSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.felt_type() == SchemaTypeId::void() {
            let mut state = serializer.serialize_struct("FeltSchema", 2)?;
            state.serialize_field("type", &SchemaTypeId::void())?;
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
        if self.felt_type() != SchemaTypeId::native_felt() {
            state.serialize_field("type", &self.felt_type())?;
        }
        if let Some(default_value) = self.default_value() {
            state.serialize_field(
                "default-value",
                &SCHEMA_TYPE_REGISTRY.display_felt(&self.felt_type(), default_value),
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
        #[serde(rename_all = "kebab-case", deny_unknown_fields)]
        struct RawFeltSchema {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, rename = "default-value")]
            default_value: Option<String>,
            #[serde(default, rename = "type")]
            r#type: Option<SchemaTypeId>,
        }

        let raw = RawFeltSchema::deserialize(deserializer)?;

        let felt_type = raw.r#type.unwrap_or_else(SchemaTypeId::native_felt);

        let description = raw.description.and_then(|description| {
            if description.trim().is_empty() {
                None
            } else {
                Some(description)
            }
        });

        if felt_type == SchemaTypeId::void() {
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
