use alloc::vec;
use core::error::Error;

use assert_matches::assert_matches;
use miden_protocol::Word;
use miden_protocol::errors::TransactionHeaderError;
use miden_protocol::note::Note;
use miden_protocol::transaction::{InputNotes, TransactionHeader};

use crate::decoded::account::test_utils::private_account_id;
use crate::{BuildUnchecked, ConversionError, DecodeMessage, Verify, proto};

#[test]
fn transaction_id_verifies() {
    let decoded = proto::transaction::TransactionId { id: Some(Word::empty().into()) }
        .decode_fields()
        .unwrap();
    assert_eq!(
        decoded.verify().unwrap(),
        miden_protocol::transaction::TransactionId::from_raw(Word::empty())
    );
}

#[test]
fn transaction_header_unchecked_build_still_checks_id_and_duplicates() {
    use miden_protocol::account::{AccountId, AccountIdVersion, AccountType, AssetCallbackFlag};

    use crate::BuildUnchecked;
    let id = AccountId::dummy(
        [7; 15],
        AccountIdVersion::Version1,
        AccountType::Private,
        AssetCallbackFlag::Disabled,
    );
    let header = miden_protocol::transaction::TransactionHeader::new(
        id,
        Word::empty(),
        Word::empty(),
        Default::default(),
        vec![],
    )
    .unwrap();
    let wire = proto::transaction::TransactionHeader::from(&header);
    assert_eq!(wire.clone().decode_fields().unwrap().build_unchecked().unwrap(), header);
    let invalid = proto::transaction::TransactionHeader {
        transaction_id: Some(proto::transaction::TransactionId {
            id: Some(Word::from([1_u32, 0, 0, 0]).into()),
        }),
        ..wire.clone()
    }
    .decode_fields()
    .unwrap();
    assert!(matches!(
        invalid.build_unchecked(),
        Err(crate::decoded::transaction::TransactionHeaderBuildError::IdMismatch { .. })
    ));
    let input = proto::transaction::InputNoteCommitment {
        nullifier: Some(Word::empty().into()),
        header: None,
    };
    let duplicate = proto::transaction::TransactionHeader {
        input_notes: vec![input.clone(), input],
        ..wire
    }
    .decode_fields()
    .unwrap();
    assert!(matches!(
        duplicate.build_unchecked(),
        Err(crate::decoded::transaction::TransactionHeaderBuildError::Input(_))
    ));
}

#[test]
fn transaction_header_conversion_preserves_validation_error_source() {
    let note = Note::mock_noop(Word::empty());
    let transaction = TransactionHeader::new(
        private_account_id(),
        Word::from([1_u32, 2, 3, 4]),
        Word::from([5_u32, 6, 7, 8]),
        InputNotes::default(),
        vec![*note.header()],
    )
    .unwrap();
    let mut message = proto::transaction::TransactionHeader::from(transaction);
    message.output_notes.push(message.output_notes[0].clone());

    let error = message
        .decode_fields()
        .unwrap()
        .build_unchecked()
        .map_err(ConversionError::new)
        .unwrap_err();
    let source = error
        .source()
        .and_then(Error::source)
        .unwrap()
        .downcast_ref::<TransactionHeaderError>()
        .unwrap();

    assert_matches!(
        source,
        TransactionHeaderError::DuplicateOutputNote(note_id) if *note_id == note.id()
    );
}
