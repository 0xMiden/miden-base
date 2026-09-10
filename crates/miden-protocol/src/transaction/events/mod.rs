//! Public transaction event records and their ordered commitment.

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::account::AccountId;
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{
    Felt,
    Hasher,
    MAX_EVENT_PAYLOAD_WORDS,
    MAX_EVENT_PAYLOAD_WORDS_PER_TX,
    MAX_EVENTS_PER_TX,
    Word,
};

// ERRORS
// ================================================================================================

/// Errors from constructing or extending a transaction's event records.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransactionEventDataError {
    /// An individual payload exceeds its word limit.
    #[error("event payload has {0} words, exceeding the maximum of {MAX_EVENT_PAYLOAD_WORDS}")]
    TooManyPayloadWords(usize),
    /// The collection exceeds its event count limit.
    #[error("transaction has {0} events, exceeding the maximum of {MAX_EVENTS_PER_TX}")]
    TooManyEvents(usize),
    /// The combined payloads exceed the transaction's word limit.
    #[error(
        "transaction event payloads have {0} words, exceeding the maximum of {MAX_EVENT_PAYLOAD_WORDS_PER_TX}"
    )]
    TooManyTotalPayloadWords(usize),
}

// TRANSACTION EVENT
// ================================================================================================

/// An event emitted by an account during a transaction.
///
/// The emitter, topic, and payload are public, including for private accounts. Construction
/// validates the payload size, not the origin of the event. Emitter attribution will be enforced
/// by the transaction kernel and bound to the transaction proof.
///
/// Empty payloads are allowed. An empty payload still represents an event with an emitter and
/// topic, rather than the absence of an event.
///
/// # Topics
///
/// Topics identify event types. The kernel does not interpret their meaning.
/// Applications can derive a topic with
/// `Hasher::hash(signature.as_bytes())`, using a case-sensitive, namespaced signature such as
/// `example::Transfer(account_id,u64)`. Signatures contain ordered parameter types, without
/// whitespace or parameter names. Applications must agree on the type names and payload schema;
/// this type does not interpret either.
// TODO(#3831): Change "will be enforced" to "is enforced" once kernel integration is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionEvent {
    emitter: AccountId,
    topic: Word,
    payload: Vec<Word>,
}

impl TransactionEvent {
    /// Creates an event with the provided emitter, topic, and payload.
    ///
    /// Returns an error if the payload exceeds [`MAX_EVENT_PAYLOAD_WORDS`].
    pub fn new(
        emitter: AccountId,
        topic: Word,
        payload: Vec<Word>,
    ) -> Result<Self, TransactionEventDataError> {
        Self::validate_payload_size(payload.len())?;
        Ok(Self { emitter, topic, payload })
    }

    /// Returns the account that emitted this event, which may be a foreign account.
    pub fn emitter(&self) -> AccountId {
        self.emitter
    }

    /// Returns the event's application-defined topic.
    pub fn topic(&self) -> Word {
        self.topic
    }

    /// Returns the event's payload words.
    pub fn payload(&self) -> &[Word] {
        &self.payload
    }

    /// Returns the number of words in the payload.
    pub fn num_payload_words(&self) -> usize {
        self.payload.len()
    }

    /// Computes the payload commitment using the field element hash, not the byte hash.
    ///
    /// The commitment of an empty payload is [`Word::empty`].
    pub fn payload_commitment(&self) -> Word {
        Hasher::hash_elements(Word::words_as_elements(&self.payload))
    }

    /// Consumes this event and returns its emitter, topic, and payload.
    pub fn into_parts(self) -> (AccountId, Word, Vec<Word>) {
        (self.emitter, self.topic, self.payload)
    }

    fn validate_payload_size(num_words: usize) -> Result<(), TransactionEventDataError> {
        if num_words > MAX_EVENT_PAYLOAD_WORDS {
            return Err(TransactionEventDataError::TooManyPayloadWords(num_words));
        }
        Ok(())
    }

    /// Checks both payload limits before the reader can allocate or read the payload words.
    fn read_with_payload_budget<R: ByteReader>(
        source: &mut R,
        previous_payload_words: usize,
    ) -> Result<Self, DeserializationError> {
        let emitter = AccountId::read_from(source)?;
        let topic = Word::read_from(source)?;
        let num_words = usize::from(source.read_u16()?);
        Self::validate_payload_size(num_words)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;
        TransactionEvents::validate_total_payload_size(previous_payload_words + num_words)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;

        let payload = source.read_many_iter::<Word>(num_words)?.collect::<Result<_, _>>()?;
        Self::new(emitter, topic, payload)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

impl Serializable for TransactionEvent {
    /// Writes the emitter, topic, payload word count as a little-endian u16, and payload words.
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.emitter.write_into(target);
        self.topic.write_into(target);
        target.write_u16(self.payload.len() as u16);
        target.write_many(&self.payload);
    }

    fn get_size_hint(&self) -> usize {
        Self::min_serialized_size() + self.payload.len() * Word::SERIALIZED_SIZE
    }
}

impl Deserializable for TransactionEvent {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        Self::read_with_payload_budget(source, 0)
    }

    fn min_serialized_size() -> usize {
        AccountId::SERIALIZED_SIZE + Word::SERIALIZED_SIZE + size_of::<u16>()
    }
}

// TRANSACTION EVENTS
// ================================================================================================

/// An ordered collection of transaction events with a cached commitment.
///
/// Empty lists and duplicate events are allowed. Appending an event preserves insertion order
/// and updates the commitment; neither the records nor the commitment can be mutated directly.
/// Finalization during execution is a kernel responsibility, not a state of this value type.
///
/// # Commitment
///
/// For events numbered `1..=n`, in field element order:
///
/// ```text
/// P_i = hash_elements(flatten(payload_i))
/// M_i = [emitter_suffix, emitter_prefix, payload_word_count, i]
/// C_0 = EMPTY_WORD
/// C_i = hash_elements_in_domain(C_(i-1) || M_i || topic_i || P_i, 0x02_0002)
/// commitment = C_n
/// ```
///
/// Each hash chain update hashes four words (two Poseidon2 rate blocks), in addition to hashing
/// the payload. The position is derived from the list, binding its count and order without
/// serializing another field. An empty list
/// has commitment [`Word::empty`]. Payload hashing uses the ordinary field element hash so its
/// commitment can also serve as an advice map key.
///
/// # Example
///
/// ```
/// use miden_protocol::account::AccountId;
/// use miden_protocol::transaction::{TransactionEvent, TransactionEvents};
/// use miden_protocol::{Felt, Hasher, Word};
///
/// let emitter = AccountId::try_from_elements(Felt::from(256u32), Felt::ONE)?;
/// let topic = Hasher::hash(b"example::Updated(word)");
/// let event = TransactionEvent::new(emitter, topic, vec![Word::from([1u32, 2, 3, 4])])?;
/// let mut events = TransactionEvents::default();
/// events.try_push(event)?;
/// assert_eq!(events.num_events(), 1);
/// assert_ne!(events.commitment(), Word::empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransactionEvents {
    events: Vec<TransactionEvent>,
    commitment: Word,
}

impl TransactionEvents {
    /// Protocol domain for the event hash chain, following the account patch (0x02_0000) and
    /// account delta (0x02_0001) domains. It occupies the second Poseidon2 capacity element.
    const DOMAIN: Felt = Felt::new_unchecked(0x02_0002);

    /// Creates a collection by appending the provided events in order.
    ///
    /// Returns an error if the count exceeds [`MAX_EVENTS_PER_TX`] or the total payload size
    /// exceeds [`MAX_EVENT_PAYLOAD_WORDS_PER_TX`].
    pub fn new(events: Vec<TransactionEvent>) -> Result<Self, TransactionEventDataError> {
        Self::validate_event_count(events.len())?;
        let mut result = Self::default();
        for event in events {
            result.try_push(event)?;
        }
        Ok(result)
    }

    /// Appends an event and updates the commitment.
    ///
    /// Returns an error if the resulting count exceeds [`MAX_EVENTS_PER_TX`] or total payload
    /// size exceeds [`MAX_EVENT_PAYLOAD_WORDS_PER_TX`]. On error the collection is unchanged.
    pub fn try_push(&mut self, event: TransactionEvent) -> Result<(), TransactionEventDataError> {
        let num_events = self.events.len() + 1;
        Self::validate_event_count(num_events)?;
        Self::validate_total_payload_size(self.num_payload_words() + event.num_payload_words())?;

        let metadata = Word::new([
            event.emitter().suffix(),
            event.emitter().prefix().as_felt(),
            Felt::from(event.num_payload_words() as u16),
            Felt::from(num_events as u16),
        ]);
        let words = [self.commitment, metadata, event.topic(), event.payload_commitment()];
        let commitment =
            Hasher::hash_elements_in_domain(Word::words_as_elements(&words), Self::DOMAIN);

        self.events.push(event);
        self.commitment = commitment;
        Ok(())
    }

    /// Returns the commitment to the ordered event list.
    pub fn commitment(&self) -> Word {
        self.commitment
    }

    /// Returns the number of events.
    pub fn num_events(&self) -> usize {
        self.events.len()
    }

    /// Returns the total number of payload words, excluding event metadata.
    pub fn num_payload_words(&self) -> usize {
        self.events.iter().map(TransactionEvent::num_payload_words).sum()
    }

    /// Returns whether the collection contains no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the events in emission order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &TransactionEvent> {
        self.events.iter()
    }

    /// Consumes this collection and returns its events in emission order.
    pub fn into_vec(self) -> Vec<TransactionEvent> {
        self.events
    }

    fn validate_event_count(num_events: usize) -> Result<(), TransactionEventDataError> {
        if num_events > MAX_EVENTS_PER_TX {
            return Err(TransactionEventDataError::TooManyEvents(num_events));
        }
        Ok(())
    }

    fn validate_total_payload_size(num_words: usize) -> Result<(), TransactionEventDataError> {
        if num_words > MAX_EVENT_PAYLOAD_WORDS_PER_TX {
            return Err(TransactionEventDataError::TooManyTotalPayloadWords(num_words));
        }
        Ok(())
    }
}

impl IntoIterator for TransactionEvents {
    type Item = TransactionEvent;
    type IntoIter = alloc::vec::IntoIter<TransactionEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

impl<'a> IntoIterator for &'a TransactionEvents {
    type Item = &'a TransactionEvent;
    type IntoIter = core::slice::Iter<'a, TransactionEvent>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.iter()
    }
}

impl Serializable for TransactionEvents {
    /// Writes a little-endian u16 event count followed by the records, without the commitment.
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u16(self.events.len() as u16);
        target.write_many(&self.events);
    }

    fn get_size_hint(&self) -> usize {
        size_of::<u16>() + self.events.iter().map(Serializable::get_size_hint).sum::<usize>()
    }
}

impl Deserializable for TransactionEvents {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_events = usize::from(source.read_u16()?);
        Self::validate_event_count(num_events)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;

        let mut events = Self::default();
        for _ in 0..num_events {
            let event =
                TransactionEvent::read_with_payload_budget(source, events.num_payload_words())?;
            events
                .try_push(event)
                .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;
        }
        Ok(events)
    }

    fn min_serialized_size() -> usize {
        size_of::<u16>()
    }
}

#[cfg(test)]
mod tests;
