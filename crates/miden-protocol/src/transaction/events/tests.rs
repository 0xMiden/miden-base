use assert_matches::assert_matches;
use rstest::rstest;

use super::*;
use crate::utils::hex_to_bytes;
use crate::utils::serde::SliceReader;

fn emitter(suffix: u32, prefix: u32) -> AccountId {
    AccountId::try_from_elements(Felt::from(suffix), Felt::from(prefix)).unwrap()
}

fn event(num_words: usize) -> TransactionEvent {
    TransactionEvent::new(
        emitter(256, 1),
        Word::from([1u32, 2, 3, 4]),
        vec![Word::from([9u32, 10, 11, 12]); num_words],
    )
    .unwrap()
}

/// Fixed private/public emitters, odd/even payload lengths, and a duplicate empty-payload event.
fn vector_events() -> Vec<TransactionEvent> {
    vec![
        event(0),
        TransactionEvent::new(
            emitter(512, 17),
            Word::from([5u32, 6, 7, 8]),
            vec![Word::from([9u32, 10, 11, 12])],
        )
        .unwrap(),
        TransactionEvent::new(
            emitter(256, 1),
            Word::from([1u32, 2, 3, 4]),
            vec![Word::from([1u32, 2, 3, 4]), Word::empty()],
        )
        .unwrap(),
        event(0),
    ]
}

#[test]
fn empty_collection() {
    let events = TransactionEvents::default();
    assert_eq!(events, TransactionEvents::new(vec![]).unwrap());
    assert!(events.is_empty());
    assert_eq!(events.num_events(), 0);
    assert_eq!(events.num_payload_words(), 0);
    assert_eq!(events.commitment(), Word::empty());
    assert_eq!(events.to_bytes(), [0, 0]);
    assert_eq!(events.get_size_hint(), 2);
    assert_eq!(TransactionEvents::read_from_bytes(&[0, 0]).unwrap(), events);
}

#[test]
fn empty_payload_is_not_an_absent_event_or_zero_word_payload() {
    let empty_payload = event(0);
    assert_eq!(empty_payload.payload_commitment(), Word::empty());

    let mut zero_word_payload = empty_payload.clone();
    zero_word_payload.payload.push(Word::empty());
    assert_ne!(zero_word_payload.payload_commitment(), Word::empty());

    let events = TransactionEvents::new(vec![empty_payload]).unwrap();
    assert!(!events.is_empty());
    assert_ne!(events.commitment(), Word::empty());
    assert_ne!(
        events.commitment(),
        TransactionEvents::new(vec![zero_word_payload]).unwrap().commitment()
    );
}

#[test]
fn construction_and_appends_preserve_records_and_commitments() {
    let records = vector_events();
    let mut events = TransactionEvents::default();
    for (index, record) in records.iter().enumerate() {
        events.try_push(record.clone()).unwrap();
        assert_eq!(events, TransactionEvents::new(records[..=index].to_vec()).unwrap());
    }

    assert_eq!(events.num_events(), records.len());
    assert_eq!(events.num_payload_words(), 3);
    assert_eq!(events.into_vec(), records);
}

#[test]
fn collection_iterators_preserve_order() {
    let records = vector_events();
    let events = TransactionEvents::new(records.clone()).unwrap();
    assert_eq!(events.iter().len(), records.len());
    assert!(events.iter().eq(&records));
    assert!((&events).into_iter().eq(&records));
    assert!(events.clone().into_iter().eq(records.clone()));
    assert_eq!(events.into_vec(), records);
}

#[test]
fn event_accessors_preserve_fields() {
    let record = event(1);
    assert_eq!(record.emitter(), emitter(256, 1));
    assert_eq!(record.topic(), Word::from([1u32, 2, 3, 4]));
    assert_eq!(record.payload(), &[Word::from([9u32, 10, 11, 12])]);
    assert_eq!(
        record.clone().into_parts(),
        (record.emitter(), record.topic(), record.payload().to_vec())
    );
}

#[test]
fn commitment_vectors() {
    // Generated independently by absorbing two rate blocks per append. Keep the intermediate
    // values so the kernel implementation can check every step against the same vectors.
    let payload_commitments = [
        "0x0000000000000000000000000000000000000000000000000000000000000000",
        "0x63889211f82b96d496810b890af36603e52d416bc913f16dfb85c2a453295c2d",
        "0xdb365b9fbe32a920a0b7396c0ba5d09fce08362e113affe99cdc51e55e6291b5",
        "0x0000000000000000000000000000000000000000000000000000000000000000",
    ];
    let commitments = [
        "0x91e4a4edcb160a6f6fcff60727a02ed32ef7e52130d57e497c7e53c9cfd4c26e",
        "0x3636cb79fa8902b701eb5850190112ce1bf9aa835b62a2a178a33e106c0fb002",
        "0x0a76351f1015db5918e7d3ae6a7cb7274bf36bb0ee1b794e651093cbff684f42",
        "0x391edcf6bcd361fd48f26c66599aef7ddcbf966328d6b2f0ee4b1b4be533e1a2",
    ];
    let mut events = TransactionEvents::default();
    for (index, record) in vector_events().into_iter().enumerate() {
        assert_eq!(
            record.payload_commitment(),
            Word::try_from(payload_commitments[index]).unwrap()
        );
        events.try_push(record).unwrap();
        assert_eq!(events.commitment(), Word::try_from(commitments[index]).unwrap());
    }
}

#[rstest]
#[case::emitter_suffix(|events: &mut Vec<TransactionEvent>| events[1].emitter = emitter(768, 17))]
#[case::emitter_prefix(|events: &mut Vec<TransactionEvent>| events[1].emitter = emitter(512, 1))]
#[case::topic(|events: &mut Vec<TransactionEvent>| events[1].topic = Word::empty())]
#[case::payload_contents(|events: &mut Vec<TransactionEvent>| events[1].payload[0] = Word::empty())]
#[case::payload_length(|events: &mut Vec<TransactionEvent>| events[1].payload.push(Word::empty()))]
#[case::order(|events: &mut Vec<TransactionEvent>| events.swap(0, 1))]
#[case::removed_event(|events: &mut Vec<TransactionEvent>| events.truncate(3))]
#[case::added_event(|events: &mut Vec<TransactionEvent>| events.push(event(0)))]
fn commitment_binds_every_field_count_and_order(#[case] mutate: fn(&mut Vec<TransactionEvent>)) {
    let records = vector_events();
    let commitment = TransactionEvents::new(records.clone()).unwrap().commitment();
    let mut changed = records;
    mutate(&mut changed);
    assert_ne!(commitment, TransactionEvents::new(changed).unwrap().commitment());
}

#[test]
fn append_uses_two_rate_blocks_with_the_event_domain() {
    let mut events = TransactionEvents::default();
    for record in vector_events() {
        let mut state = [Felt::ZERO; Hasher::STATE_WIDTH];
        state[Hasher::CAPACITY_RANGE.start + 1] = TransactionEvents::DOMAIN;
        state[..4].copy_from_slice(events.commitment().as_elements());
        state[4..8].copy_from_slice(&[
            record.emitter().suffix(),
            record.emitter().prefix().as_felt(),
            Felt::from(record.num_payload_words() as u16),
            Felt::from((events.num_events() + 1) as u16),
        ]);
        Hasher::apply_permutation(&mut state);
        state[..4].copy_from_slice(record.topic().as_elements());
        state[4..8].copy_from_slice(record.payload_commitment().as_elements());
        Hasher::apply_permutation(&mut state);

        events.try_push(record).unwrap();
        assert_eq!(events.commitment().as_elements(), &state[Hasher::DIGEST_RANGE]);
    }
}

#[test]
fn serialization_vector() {
    let bytes = hex_to_bytes::<294>(concat!(
        "0x0400",
        "000000000000000100000000000001",
        "0100000000000000020000000000000003000000000000000400000000000000",
        "0000",
        "000000000000001100000000000002",
        "0500000000000000060000000000000007000000000000000800000000000000",
        "0100",
        "09000000000000000a000000000000000b000000000000000c00000000000000",
        "000000000000000100000000000001",
        "0100000000000000020000000000000003000000000000000400000000000000",
        "0200",
        "0100000000000000020000000000000003000000000000000400000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "000000000000000100000000000001",
        "0100000000000000020000000000000003000000000000000400000000000000",
        "0000",
    ))
    .unwrap();

    let events = TransactionEvents::new(vector_events()).unwrap();
    assert_eq!(events.to_bytes(), bytes);
    assert_eq!(events.get_size_hint(), bytes.len());
    assert_eq!(TransactionEvents::read_from_bytes(&bytes).unwrap(), events);
    assert_eq!(TransactionEvent::min_serialized_size(), 49);
    assert_eq!(TransactionEvents::min_serialized_size(), 2);
}

#[rstest]
#[case(0)]
#[case(1)]
#[case(2)]
#[case(MAX_EVENT_PAYLOAD_WORDS)]
fn event_serialization(#[case] num_words: usize) {
    let record = event(num_words);
    let bytes = record.to_bytes();
    assert_eq!(record.get_size_hint(), bytes.len());
    assert_eq!(TransactionEvent::read_from_bytes(&bytes).unwrap(), record);
    assert_eq!(record.payload().len(), num_words);
}

#[test]
fn maximum_collection_serialization() {
    let full_payloads = MAX_EVENT_PAYLOAD_WORDS_PER_TX / MAX_EVENT_PAYLOAD_WORDS;
    let mut records = vec![event(MAX_EVENT_PAYLOAD_WORDS); full_payloads];
    records.resize(MAX_EVENTS_PER_TX, event(0));
    let events = TransactionEvents::new(records).unwrap();
    assert_eq!(events.num_events(), MAX_EVENTS_PER_TX);
    assert_eq!(events.num_payload_words(), MAX_EVENT_PAYLOAD_WORDS_PER_TX);
    let bytes = events.to_bytes();
    assert_eq!(events.get_size_hint(), bytes.len());
    assert_eq!(TransactionEvents::read_from_bytes(&bytes).unwrap(), events);
}

#[test]
fn constructors_reject_excessive_sizes() {
    assert_eq!(
        TransactionEvent::new(
            emitter(256, 1),
            Word::empty(),
            vec![Word::empty(); MAX_EVENT_PAYLOAD_WORDS + 1],
        )
        .unwrap_err(),
        TransactionEventDataError::TooManyPayloadWords(MAX_EVENT_PAYLOAD_WORDS + 1)
    );
    assert_eq!(
        TransactionEvents::new(vec![event(0); MAX_EVENTS_PER_TX + 1]).unwrap_err(),
        TransactionEventDataError::TooManyEvents(MAX_EVENTS_PER_TX + 1)
    );

    let mut records = vec![
        event(MAX_EVENT_PAYLOAD_WORDS);
        MAX_EVENT_PAYLOAD_WORDS_PER_TX / MAX_EVENT_PAYLOAD_WORDS
    ];
    records.push(event(1));
    assert_eq!(
        TransactionEvents::new(records).unwrap_err(),
        TransactionEventDataError::TooManyTotalPayloadWords(MAX_EVENT_PAYLOAD_WORDS_PER_TX + 1)
    );
}

#[test]
fn failed_appends_leave_the_collection_unchanged() {
    let mut events = TransactionEvents::new(vec![event(0); MAX_EVENTS_PER_TX]).unwrap();
    let before = events.clone();
    assert_eq!(
        events.try_push(event(0)).unwrap_err(),
        TransactionEventDataError::TooManyEvents(MAX_EVENTS_PER_TX + 1)
    );
    assert_eq!(events, before);

    let mut events = TransactionEvents::new(vec![
        event(MAX_EVENT_PAYLOAD_WORDS);
        MAX_EVENT_PAYLOAD_WORDS_PER_TX
            / MAX_EVENT_PAYLOAD_WORDS
    ])
    .unwrap();
    let before = events.clone();
    assert_eq!(
        events.try_push(event(1)).unwrap_err(),
        TransactionEventDataError::TooManyTotalPayloadWords(MAX_EVENT_PAYLOAD_WORDS_PER_TX + 1)
    );
    assert_eq!(events, before);
    events.try_push(event(0)).unwrap();
}

fn assert_invalid_value(error: DeserializationError, expected: TransactionEventDataError) {
    assert_matches!(error, DeserializationError::InvalidValue(message) => {
        assert_eq!(message, expected.to_string());
    });
}

#[test]
fn decoder_rejects_event_count_before_reading_records() {
    let mut bytes = Vec::new();
    bytes.write_u16((MAX_EVENTS_PER_TX + 1) as u16);
    bytes.write_u8(0xab);
    let mut reader = SliceReader::new(&bytes);
    assert_invalid_value(
        TransactionEvents::read_from(&mut reader).unwrap_err(),
        TransactionEventDataError::TooManyEvents(MAX_EVENTS_PER_TX + 1),
    );
    assert_eq!(reader.read_u8().unwrap(), 0xab);
}

#[rstest]
#[case::standalone(false)]
#[case::collection(true)]
fn decoder_rejects_payload_length_before_reading_payload(#[case] collection: bool) {
    let mut bytes = Vec::new();
    if collection {
        bytes.write_u16(1);
    }
    bytes.write(emitter(256, 1));
    bytes.write(Word::empty());
    bytes.write_u16((MAX_EVENT_PAYLOAD_WORDS + 1) as u16);
    bytes.write_u8(0xab);
    let mut reader = SliceReader::new(&bytes);
    let error = if collection {
        TransactionEvents::read_from(&mut reader).unwrap_err()
    } else {
        TransactionEvent::read_from(&mut reader).unwrap_err()
    };
    assert_invalid_value(
        error,
        TransactionEventDataError::TooManyPayloadWords(MAX_EVENT_PAYLOAD_WORDS + 1),
    );
    assert_eq!(reader.read_u8().unwrap(), 0xab);
}

#[test]
fn decoder_rejects_total_length_before_reading_excess_payload() {
    let full_payloads = MAX_EVENT_PAYLOAD_WORDS_PER_TX / MAX_EVENT_PAYLOAD_WORDS;
    let mut bytes = Vec::new();
    bytes.write_u16((full_payloads + 1) as u16);
    for _ in 0..full_payloads {
        bytes.write(event(MAX_EVENT_PAYLOAD_WORDS));
    }
    bytes.write(emitter(256, 1));
    bytes.write(Word::empty());
    bytes.write_u16(1);
    bytes.write_u8(0xab);
    let mut reader = SliceReader::new(&bytes);
    assert_invalid_value(
        TransactionEvents::read_from(&mut reader).unwrap_err(),
        TransactionEventDataError::TooManyTotalPayloadWords(MAX_EVENT_PAYLOAD_WORDS_PER_TX + 1),
    );
    assert_eq!(reader.read_u8().unwrap(), 0xab);
}

#[test]
fn decoder_rejects_invalid_account_id() {
    let mut bytes = event(0).to_bytes();
    bytes[..AccountId::SERIALIZED_SIZE].fill(0);
    assert_matches!(
        TransactionEvent::read_from_bytes(&bytes),
        Err(DeserializationError::InvalidValue(_))
    );
}

#[rstest]
#[case::topic(AccountId::SERIALIZED_SIZE)]
#[case::payload(TransactionEvent::min_serialized_size())]
fn decoder_rejects_noncanonical_field_elements(#[case] offset: usize) {
    let mut bytes = event(1).to_bytes();
    bytes[offset..offset + 8].copy_from_slice(&Felt::ORDER.to_le_bytes());
    assert_matches!(
        TransactionEvent::read_from_bytes(&bytes),
        Err(DeserializationError::InvalidValue(_))
    );
}

#[test]
fn decoder_rejects_truncated_records_and_collections() {
    let bytes = event(2).to_bytes();
    for len in 0..bytes.len() {
        assert!(TransactionEvent::read_from_bytes(&bytes[..len]).is_err(), "length {len}");
    }
    let bytes = TransactionEvents::new(vector_events()).unwrap().to_bytes();
    for len in 0..bytes.len() {
        assert!(TransactionEvents::read_from_bytes(&bytes[..len]).is_err(), "length {len}");
    }
}
