use alloc::string::ToString;
use core::error::Error;

use miden_core::{Felt, FieldElement, Word};

use crate::account::component::toml::init_storage_data::InitStorageDataError;
use crate::account::component::{
    AccountComponentMetadata,
    InitStorageData,
    SchemaTypeIdentifier,
    StorageSlotSchema,
    StorageValueName,
    WordSchema,
    WordValue,
};
use crate::account::{AccountStorage, StorageSlotContent, StorageSlotName};
use crate::asset::TokenSymbol;
use crate::errors::AccountComponentTemplateError;

#[test]
fn from_toml_str_with_nested_table_and_flattened() {
    let toml_table = r#"
        [token_metadata]
        max_supply = "1000000000"
        symbol = "ETH"
        decimals = "9"
    "#;

    let toml_inline = r#"
        token_metadata.max_supply = "1000000000"
        token_metadata.symbol = "ETH"
        token_metadata.decimals = "9"
    "#;

    let storage_table = InitStorageData::from_toml(toml_table).unwrap();
    let storage_inline = InitStorageData::from_toml(toml_inline).unwrap();

    assert_eq!(storage_table.values(), storage_inline.values());
}

#[test]
fn from_toml_str_with_deeply_nested_tables() {
    let toml_str = r#"
        [a]
        b = "0xb"

        [a.c]
        d = "0xd"

        [x.y.z]
        w = 42 # NOTE: This gets parsed as string
    "#;

    let storage = InitStorageData::from_toml(toml_str).expect("Failed to parse TOML");
    let key1: StorageValueName = "a.b".parse().unwrap();
    let key2: StorageValueName = "a.c.d".parse().unwrap();
    let key3: StorageValueName = "x.y.z.w".parse().unwrap();

    let WordValue::Scalar(v1) = storage.get(&key1).unwrap() else {
        panic!("expected a raw value for {key1}");
    };
    let WordValue::Scalar(v2) = storage.get(&key2).unwrap() else {
        panic!("expected a raw value for {key2}");
    };
    let WordValue::Scalar(v3) = storage.get(&key3).unwrap() else {
        panic!("expected a raw value for {key3}");
    };

    assert_eq!(v1, "0xb");
    assert_eq!(v2, "0xd");
    assert_eq!(v3, "42");
}

#[test]
fn test_error_on_array() {
    let toml_str = r#"
        token_metadata.v = [1, 2, 3]
    "#;

    let result = InitStorageData::from_toml(toml_str);
    assert_matches::assert_matches!(result.unwrap_err(), InitStorageDataError::ArraysNotSupported);
}

#[test]
fn parse_map_entries_from_array() {
    let toml_str = r#"
        my_map = [
            { key = "0x0000000000000000000000000000000000000000000000000000000000000001", value = "0x0000000000000000000000000000000000000000000000000000000000000010" },
            { key = "0x0000000000000000000000000000000000000000000000000000000000000002", value = ["1", "2", "3", "4"] }
        ]
    "#;

    let storage = InitStorageData::from_toml(toml_str).expect("Failed to parse map entries");
    let map_name: StorageValueName = "my_map".parse().unwrap();
    let entries = storage.map_entries(&map_name).expect("map entries missing");
    assert_eq!(entries.len(), 2);

    assert_matches::assert_matches!(
        &entries[0].0,
        WordValue::Scalar(v)
            if v == "0x0000000000000000000000000000000000000000000000000000000000000001"
    );
    assert_matches::assert_matches!(
        &entries[0].1,
        WordValue::Scalar(v)
            if v == "0x0000000000000000000000000000000000000000000000000000000000000010"
    );
    assert_matches::assert_matches!(&entries[1].1, WordValue::Elements(elements) if elements == &[
        "1".to_string(),
        "2".to_string(),
        "3".to_string(),
        "4".to_string(),
    ]);
}

#[test]
fn error_on_empty_subtable() {
    let toml_str = r#"
        [a]
        b = {}
    "#;

    let result = InitStorageData::from_toml(toml_str);
    assert_matches::assert_matches!(result.unwrap_err(), InitStorageDataError::EmptyTable(_));
}

#[test]
fn error_on_duplicate_keys() {
    let toml_str = r#"
        token_metadata.max_supply = "1000000000"
        token_metadata.max_supply = "500000000"
    "#;

    let result = InitStorageData::from_toml(toml_str).unwrap_err();
    // TOML does not support duplicate keys
    assert_matches::assert_matches!(result, InitStorageDataError::InvalidToml(_));
    assert!(result.source().unwrap().to_string().contains("duplicate"));
}

#[test]
fn metadata_from_toml_parses_named_storage_schema() {
    let toml_str = r#"
        name = "Test Component"
        description = "Test description"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::test_value"
        description = "a demo slot"
        type = "word"

        [[storage]]
        name = "demo::my_map"
        type = "map"
    "#;

    let metadata = AccountComponentMetadata::from_toml(toml_str).unwrap();
    let requirements = metadata.init_value_requirements();

    assert!(requirements.contains_key(&"demo::test_value".parse::<StorageValueName>().unwrap()));
    assert!(!requirements.contains_key(&"demo::my_map".parse::<StorageValueName>().unwrap()));
}

#[test]
fn metadata_from_toml_rejects_typed_fields_in_static_map_values() {
    let toml_str = r#"
        name = "Test Component"
        description = "Test description"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::my_map"
        type = "map"
        default-values = [
            { key = "0x1", value = { type = "word" } },
        ]
    "#;

    assert_matches::assert_matches!(
        AccountComponentMetadata::from_toml(toml_str),
        Err(AccountComponentTemplateError::TomlDeserializationError(_))
    );
}

#[test]
fn metadata_from_toml_rejects_reserved_slot_names() {
    let reserved_slot = AccountStorage::faucet_sysdata_slot().as_str();

    let toml_str = format!(
        r#"
            name = "Test Component"
            description = "Test description"
            version = "0.1.0"
            supported-types = []

            [[storage]]
            name = "{reserved_slot}"
            type = "word"
        "#
    );

    assert_matches::assert_matches!(
        AccountComponentMetadata::from_toml(&toml_str),
        Err(AccountComponentTemplateError::ReservedSlotName(_))
    );
}

#[test]
fn metadata_toml_round_trip_value_and_map_slots() {
    let toml_str = r#"
        name = "round trip"
        description = "test round-trip"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::scalar"
        description = "single word slot"
        default-value = "0x1"

        [[storage]]
        name = "demo::statemap"
        type = "map"
        default-values = [
            { key = "0x000000000000ed5d", value = "0x10" },
        ]
    "#;

    let original =
        AccountComponentMetadata::from_toml(toml_str).expect("original metadata should parse");
    let round_trip_toml = original.to_toml().expect("serialize to toml");
    let round_trip =
        AccountComponentMetadata::from_toml(&round_trip_toml).expect("round-trip parse");

    assert_eq!(original, round_trip);
}

#[test]
fn metadata_toml_round_trip_composed_slot_with_typed_fields() {
    let toml_str = r#"
        name = "round trip typed fields"
        description = "test composed slot typed fields"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::composed"
        description = "composed word with typed fields"
        type = [
            { name = "a", type = "u16" },
            { name = "b", default-value = "0x2" },
            { name = "c", default-value = "0x3" },
            { name = "d", default-value = "0x4" },
        ]
    "#;

    let original =
        AccountComponentMetadata::from_toml(toml_str).expect("original metadata should parse");

    let mut requirements = original.init_value_requirements();
    assert_eq!(
        requirements
            .remove(&"demo::composed.a".parse::<StorageValueName>().unwrap())
            .unwrap()
            .r#type,
        SchemaTypeIdentifier::new("u16").unwrap()
    );

    let round_trip_toml = original.to_toml().expect("serialize to toml");
    let round_trip =
        AccountComponentMetadata::from_toml(&round_trip_toml).expect("round-trip parse");

    assert_eq!(original, round_trip);
}

#[test]
fn metadata_toml_round_trip_typed_slots() {
    let toml_str = r#"
        name = "typed components"
        description = "test typed slots"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::typed_value"
        type = "word"

        [[storage]]
        name = "demo::typed_map"
        type = "map"
        key-type = "auth::rpo_falcon512::pub_key"
        value-type = "auth::rpo_falcon512::pub_key"
    "#;

    let metadata =
        AccountComponentMetadata::from_toml(toml_str).expect("typed metadata should parse");
    let schema = metadata.storage_schema();

    let value_slot = schema
        .fields()
        .get(&StorageSlotName::new("demo::typed_value").unwrap())
        .expect("value slot missing");
    let value_slot = match value_slot {
        StorageSlotSchema::Value(slot) => slot,
        _ => panic!("expected value slot"),
    };

    let typed_value = SchemaTypeIdentifier::native_word();
    assert_eq!(value_slot.word(), &WordSchema::new_singular(typed_value.clone()));

    let map_slot = schema
        .fields()
        .get(&StorageSlotName::new("demo::typed_map").unwrap())
        .expect("map slot missing");
    let map_slot = match map_slot {
        StorageSlotSchema::Map(slot) => slot,
        _ => panic!("expected map slot"),
    };

    let pub_key_type = SchemaTypeIdentifier::new("auth::rpo_falcon512::pub_key").unwrap();
    assert_eq!(map_slot.key_schema(), &WordSchema::new_singular(pub_key_type.clone()));
    assert_eq!(map_slot.value_schema(), &WordSchema::new_singular(pub_key_type));

    let mut requirements = metadata.init_value_requirements();
    assert_eq!(
        requirements
            .remove(&"demo::typed_value".parse::<StorageValueName>().unwrap())
            .unwrap()
            .r#type,
        typed_value
    );
    assert!(!requirements.contains_key(&"demo::typed_map".parse::<StorageValueName>().unwrap()));

    let round_trip = metadata.to_toml().expect("serialize");
    let parsed: toml::Value = toml::from_str(&round_trip).unwrap();
    let storage = parsed.get("storage").unwrap().as_array().unwrap();

    let typed_value_entry = storage
        .iter()
        .find(|entry| entry.get("name").unwrap().as_str().unwrap() == "demo::typed_value")
        .unwrap();
    assert_eq!(typed_value_entry.get("type").unwrap().as_str().unwrap(), "word");

    let typed_map_entry = storage
        .iter()
        .find(|entry| entry.get("name").unwrap().as_str().unwrap() == "demo::typed_map")
        .unwrap();
    assert_eq!(typed_map_entry.get("type").unwrap().as_str().unwrap(), "map");
    assert_eq!(
        typed_map_entry.get("key-type").unwrap().as_str().unwrap(),
        "auth::rpo_falcon512::pub_key"
    );
    assert_eq!(
        typed_map_entry.get("value-type").unwrap().as_str().unwrap(),
        "auth::rpo_falcon512::pub_key"
    );
}

#[test]
fn extensive_schema_metadata_and_init_toml_example() {
    let metadata_toml = r#"
        name = "Extensive Example"
        description = "Exercises composed slots, singular typed slots, static maps, optional init maps, and map typing."
        version = "0.1.0"
        supported-types = ["FungibleFaucet", "RegularAccountImmutableCode"]

        # composed slot schema expressed via `type = [...]`
        [[storage]]
        name = "demo::token_metadata"
        description = "Token metadata: max_supply, symbol, decimals, reserved."
        type = [
            { type = "u32", name = "max_supply", description = "Maximum supply (base units)" },
            { type = "token_symbol", name = "symbol", default-value = "TST" },
            { type = "u8", name = "decimals", description = "Token decimals" },
            { type = "void" }
        ]

        # singular word-typed slot (must be passed at instantiation)
        [[storage]]
        name = "demo::owner_pub_key"
        description = "Owner public key"
        type = "auth::rpo_falcon512::pub_key"

        # singular felt-typed word slot (parsed as felt, stored as [0,0,0,<felt>])
        [[storage]]
        name = "demo::protocol_version"
        description = "Protocol version stored as u8 in the last felt"
        type = "u8"

        # word slot with an overridable default
        [[storage]]
        name = "demo::static_word"
        description = "A fully specified word slot"
        type = "word"
        default-value = ["0x1", "0x2", "0x3", "0x4"]

        # Word slot with implied `type = "word"`
        [[storage]]
        name = "demo::legacy_word"
        default-value = "0x123"

        # Static map defaults (fully concrete key/value words)
        [[storage]]
        name = "demo::static_map"
        description = "Static map with default entries"
        type = "map"
        default-values = [
            { key = "0x1", value = "0x10" },
            { key = ["0", "0", "0", "2"], value = ["0", "0", "0", "32"] }
        ]

        # Map with no key/value typing specified: defaults to word/word
        [[storage]]
        name = "demo::default_typed_map"
        description = "Defaults to key/value type => word/word"
        type = "map"

        # init-populated map with key/value types
        [[storage]]
        name = "demo::typed_map_new"
        type = "map"
        key-type = [
            { type = "felt", name = "prefix" },
            { type = "felt", name = "suffix" },
            { type = "void" },
            { type = "void" }
        ]
        value-type = "u16"
    "#;

    let metadata = AccountComponentMetadata::from_toml(metadata_toml).unwrap();

    // TOML round-trips
    let round_trip_toml = metadata.to_toml().unwrap();
    let round_trip = AccountComponentMetadata::from_toml(&round_trip_toml).unwrap();
    assert_eq!(metadata, round_trip);

    // map typing defaults to word/word when omitted
    let default_map_name = StorageSlotName::new("demo::default_typed_map").unwrap();
    let StorageSlotSchema::Map(default_map) =
        metadata.storage_schema().fields().get(&default_map_name).unwrap()
    else {
        panic!("expected map slot schema");
    };
    assert_eq!(
        default_map.key_schema(),
        &WordSchema::new_singular(SchemaTypeIdentifier::native_word())
    );
    assert_eq!(
        default_map.value_schema(),
        &WordSchema::new_singular(SchemaTypeIdentifier::native_word())
    );

    // `key-type`/`value-type` parse as schema/type descriptors (not literal words).
    let typed_map_new_name = StorageSlotName::new("demo::typed_map_new").unwrap();
    let StorageSlotSchema::Map(typed_map_new) =
        metadata.storage_schema().fields().get(&typed_map_new_name).unwrap()
    else {
        panic!("expected map slot schema");
    };
    assert_eq!(
        typed_map_new.value_schema(),
        &WordSchema::new_singular(SchemaTypeIdentifier::new("u16").unwrap())
    );
    assert!(matches!(typed_map_new.key_schema(), WordSchema::Composed { .. }));

    // used storage slots
    let requirements = metadata.init_value_requirements();
    assert!(requirements.contains_key(&"demo::owner_pub_key".parse::<StorageValueName>().unwrap()));
    assert!(
        requirements.contains_key(&"demo::protocol_version".parse::<StorageValueName>().unwrap())
    );
    assert!(
        requirements
            .contains_key(&"demo::token_metadata.max_supply".parse::<StorageValueName>().unwrap())
    );
    assert!(
        requirements
            .contains_key(&"demo::token_metadata.decimals".parse::<StorageValueName>().unwrap())
    );
    assert!(
        !requirements
            .contains_key(&"demo::token_metadata.symbol".parse::<StorageValueName>().unwrap())
    );
    assert!(
        !requirements.contains_key(&"demo::typed_map_new".parse::<StorageValueName>().unwrap())
    );
    assert!(!requirements.contains_key(&"demo::static_map".parse::<StorageValueName>().unwrap()));

    // Build storage without providing optional defaulted fields.
    let init_toml_defaults = r#"
        "demo::owner_pub_key" = "0x1234"
        "demo::protocol_version" = 7

        "demo::token_metadata.max_supply" = 1000000
        "demo::token_metadata.decimals" = 6
    "#;
    let init_defaults = InitStorageData::from_toml(init_toml_defaults).unwrap();
    let slots = metadata.storage_schema().build_storage_slots(&init_defaults).unwrap();

    let token_metadata_name = StorageSlotName::new("demo::token_metadata").unwrap();
    let token_metadata_slot = slots.iter().find(|s| s.name() == &token_metadata_name).unwrap();
    let StorageSlotContent::Value(token_metadata_word) = token_metadata_slot.content() else {
        panic!("expected value slot for token_metadata");
    };
    let symbol_felt: Felt = TokenSymbol::new("TST").unwrap().into();
    let expected_token_metadata =
        Word::from([Felt::from(1_000_000u32), symbol_felt, Felt::from(6u8), Felt::ZERO]);
    assert_eq!(token_metadata_word, &expected_token_metadata);

    let owner_pub_key_name = StorageSlotName::new("demo::owner_pub_key").unwrap();
    let owner_pub_key_slot = slots.iter().find(|s| s.name() == &owner_pub_key_name).unwrap();
    let StorageSlotContent::Value(owner_pub_key_word) = owner_pub_key_slot.content() else {
        panic!("expected value slot for owner_pub_key");
    };
    let expected_pub_key =
        Word::parse("0x0000000000000000000000000000000000000000000000000000000000001234").unwrap();
    assert_eq!(owner_pub_key_word, &expected_pub_key);

    let protocol_version_name = StorageSlotName::new("demo::protocol_version").unwrap();
    let protocol_version_slot = slots.iter().find(|s| s.name() == &protocol_version_name).unwrap();
    let StorageSlotContent::Value(protocol_version_word) = protocol_version_slot.content() else {
        panic!("expected value slot for protocol_version");
    };
    assert_eq!(
        protocol_version_word,
        &Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::from(7u8)])
    );

    let static_word_name = StorageSlotName::new("demo::static_word").unwrap();
    let static_word_slot = slots.iter().find(|s| s.name() == &static_word_name).unwrap();
    let StorageSlotContent::Value(static_word) = static_word_slot.content() else {
        panic!("expected value slot for static_word");
    };
    assert_eq!(
        static_word,
        &Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)])
    );

    let legacy_word_name = StorageSlotName::new("demo::legacy_word").unwrap();
    let legacy_word_slot = slots.iter().find(|s| s.name() == &legacy_word_name).unwrap();
    let StorageSlotContent::Value(legacy_word) = legacy_word_slot.content() else {
        panic!("expected value slot for legacy_word");
    };
    assert_eq!(legacy_word, &Word::parse("0x123").unwrap());

    let static_map_name = StorageSlotName::new("demo::static_map").unwrap();
    let static_map_slot = slots.iter().find(|s| s.name() == &static_map_name).unwrap();
    let StorageSlotContent::Map(static_map) = static_map_slot.content() else {
        panic!("expected map slot for static_map");
    };
    assert_eq!(static_map.num_entries(), 2);
    assert_eq!(static_map.get(&Word::parse("0x1").unwrap()), Word::parse("0x10").unwrap());
    assert_eq!(
        static_map.get(&Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::new(2)])),
        Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::new(32)])
    );

    let typed_map_new_slot = slots.iter().find(|s| s.name() == &typed_map_new_name).unwrap();
    let StorageSlotContent::Map(typed_map_new_contents) = typed_map_new_slot.content() else {
        panic!("expected map slot for typed_map_new");
    };
    assert_eq!(typed_map_new_contents.num_entries(), 0);

    // Provide init-populated multiple map entries  and rebuild.
    let init_toml_with_overrides = r#"
        "demo::owner_pub_key" = "0x1234"
        "demo::protocol_version" = 7
        "demo::legacy_word" = "0x456"

        "demo::typed_map_new" = [
          { key = ["1", "2", "0", "0"], value = "16" },
          { key = ["3", "4", "0", "0"], value = "32" }
        ]

        "demo::static_map" = [
          { key = "0x1", value = "0x99" }, # overrides default
          { key = "0x3", value = "0x30" }  # adds a new key
        ]

        ["demo::token_metadata"]
        max_supply = 1000000
        decimals = 6
        symbol = "BTC"
    "#;
    let init_with_overrides = InitStorageData::from_toml(init_toml_with_overrides).unwrap();
    let parsed_entries = init_with_overrides
        .map_entries(&"demo::typed_map_new".parse::<StorageValueName>().unwrap())
        .expect("demo::typed_map_new map entries missing");
    assert_eq!(parsed_entries.len(), 2);
    let slots_with_maps =
        metadata.storage_schema().build_storage_slots(&init_with_overrides).unwrap();

    let typed_map_new_slot =
        slots_with_maps.iter().find(|s| s.name() == &typed_map_new_name).unwrap();
    let StorageSlotContent::Map(typed_map_new_contents) = typed_map_new_slot.content() else {
        panic!("expected map slot for typed_map_new");
    };
    assert_eq!(typed_map_new_contents.num_entries(), 2);

    let key1 = Word::from([Felt::new(1), Felt::new(2), Felt::ZERO, Felt::ZERO]);
    assert_eq!(
        typed_map_new_contents.get(&key1),
        Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::new(16)])
    );

    let token_metadata_slot =
        slots_with_maps.iter().find(|s| s.name() == &token_metadata_name).unwrap();
    let StorageSlotContent::Value(token_metadata_word) = token_metadata_slot.content() else {
        panic!("expected value slot for token_metadata");
    };
    let symbol_felt: Felt = TokenSymbol::new("BTC").unwrap().into();
    let expected_token_metadata_overridden =
        Word::from([Felt::from(1_000_000u32), symbol_felt, Felt::from(6u8), Felt::ZERO]);
    assert_eq!(token_metadata_word, &expected_token_metadata_overridden);

    let legacy_word_slot = slots_with_maps.iter().find(|s| s.name() == &legacy_word_name).unwrap();
    let StorageSlotContent::Value(legacy_word) = legacy_word_slot.content() else {
        panic!("expected value slot for legacy_word");
    };
    assert_eq!(legacy_word, &Word::parse("0x456").unwrap());

    let static_map_slot = slots_with_maps.iter().find(|s| s.name() == &static_map_name).unwrap();
    let StorageSlotContent::Map(static_map) = static_map_slot.content() else {
        panic!("expected map slot for static_map");
    };
    assert_eq!(static_map.num_entries(), 3);
    assert_eq!(static_map.get(&Word::parse("0x1").unwrap()), Word::parse("0x99").unwrap());
    assert_eq!(
        static_map.get(&Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::new(2)])),
        Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::new(32)])
    );
    assert_eq!(static_map.get(&Word::parse("0x3").unwrap()), Word::parse("0x30").unwrap());
}

#[test]
fn typed_map_init_entries_are_validated() {
    let metadata_toml = r#"
        name = "typed map validation"
        description = "validates init-provided map entries against key-type/value-type"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::typed_map"
        type = "map"
        key-type = [
            { name = "prefix" },
            { name = "suffix" },
            { type = "void" },
            { type = "void" }
        ]
        value-type = "u16"
    "#;

    let metadata = AccountComponentMetadata::from_toml(metadata_toml).unwrap();

    // Key schema requires the last 2 elements to be `void` (i.e. 0).
    let init_toml = r#"
        "demo::typed_map" = [
          { key = ["1", "2", "3", "0"], value = ["0", "0", "0", "1"] }
        ]
    "#;
    let init_data = InitStorageData::from_toml(init_toml).unwrap();

    assert_matches::assert_matches!(
        metadata.storage_schema().build_storage_slots(&init_data),
        Err(AccountComponentTemplateError::InvalidInitStorageValue(name, msg))
            if name.as_str() == "demo::typed_map" && msg.contains("void")
    );
}

#[test]
fn typed_map_supports_non_numeric_value_types() {
    let metadata_toml = r#"
        name = "typed map token_symbol"
        description = "parses typed map values using the slot-level value-type"
        version = "0.1.0"
        supported-types = []

        [[storage]]
        name = "demo::symbol_map"
        type = "map"
        value-type = "token_symbol"
    "#;

    let metadata = AccountComponentMetadata::from_toml(metadata_toml).unwrap();

    let init_toml = r#"
        "demo::symbol_map" = [
            { key = "0x1", value = "BTC" }
        ]
    "#;
    let init_data = InitStorageData::from_toml(init_toml).unwrap();

    let slots = metadata.storage_schema().build_storage_slots(&init_data).unwrap();
    let slot_name = StorageSlotName::new("demo::symbol_map").unwrap();
    let slot = slots.iter().find(|s| s.name() == &slot_name).unwrap();

    let StorageSlotContent::Map(map) = slot.content() else {
        panic!("expected map slot");
    };

    assert_eq!(map.num_entries(), 1);

    let key = Word::parse("0x1").unwrap();
    let symbol_felt: Felt = TokenSymbol::new("BTC").unwrap().into();
    let expected_value = Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, symbol_felt]);
    assert_eq!(map.get(&key), expected_value);
}
