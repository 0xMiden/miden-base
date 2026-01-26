use miden_protocol::Word;
use miden_protocol::account::AccountId;
use miden_protocol::errors::{AccountIdError, NoteError};
use miden_protocol::note::{
    NoteAttachment,
    NoteAttachmentContent,
    NoteAttachmentKind,
    NoteAttachmentScheme,
    NoteExecutionHint,
};

use crate::note::WellKnownNoteAttachment;

// NETWORK ACCOUNT TARGET
// ================================================================================================

/// A [`NoteAttachment`] for notes targeted at network accounts.
///
/// It can be encoded to and from a [`NoteAttachmentContent::Word`] with the following layout:
///
/// ```text
/// - 0th felt: [target_id_suffix (56 bits) | 8 zero bits]
/// - 1st felt: [target_id_prefix (64 bits)]
/// - 2nd felt: [24 zero bits | exec_hint_payload (32 bits) | exec_hint_tag (8 bits)]
/// - 3rd felt: [64 zero bits]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkAccountTarget {
    target_id: AccountId,
    exec_hint: NoteExecutionHint,
}

impl NetworkAccountTarget {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The standardized scheme of [`NetworkAccountTarget`] attachments.
    pub const ATTACHMENT_SCHEME: NoteAttachmentScheme =
        WellKnownNoteAttachment::NetworkAccountTarget.attachment_scheme();

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NetworkAccountTarget`] from the provided parts.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the provided `target_id` does not have
    ///   [`AccountStorageMode::Network`](miden_protocol::account::AccountStorageMode::Network).
    pub fn new(
        target_id: AccountId,
        exec_hint: NoteExecutionHint,
    ) -> Result<Self, NetworkAccountTargetError> {
        // TODO: Once AccountStorageMode::Network is removed, this should check is_public.
        if !target_id.is_network() {
            return Err(NetworkAccountTargetError::TargetNotNetwork(target_id));
        }

        Ok(Self { target_id, exec_hint })
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`AccountId`] at which the note is targeted.
    pub fn target_id(&self) -> AccountId {
        self.target_id
    }

    /// Returns the [`NoteExecutionHint`] of the note.
    pub fn execution_hint(&self) -> NoteExecutionHint {
        self.exec_hint
    }
}

impl From<NetworkAccountTarget> for NoteAttachment {
    fn from(network_attachment: NetworkAccountTarget) -> Self {
        let mut word = Word::empty();
        word[0] = network_attachment.target_id.suffix();
        word[1] = network_attachment.target_id.prefix().as_felt();
        word[2] = network_attachment.exec_hint.into();

        NoteAttachment::new_word(NetworkAccountTarget::ATTACHMENT_SCHEME, word)
    }
}

impl TryFrom<&NoteAttachment> for NetworkAccountTarget {
    type Error = NetworkAccountTargetError;

    fn try_from(attachment: &NoteAttachment) -> Result<Self, Self::Error> {
        if attachment.attachment_scheme() != Self::ATTACHMENT_SCHEME {
            return Err(NetworkAccountTargetError::AttachmentSchemeMismatch(
                attachment.attachment_scheme(),
            ));
        }

        match attachment.content() {
            NoteAttachmentContent::Word(word) => {
                let id_suffix = word[0];
                let id_prefix = word[1];
                let exec_hint = word[2];

                let target_id = AccountId::try_from([id_prefix, id_suffix])
                    .map_err(NetworkAccountTargetError::DecodeTargetId)?;

                let exec_hint = NoteExecutionHint::try_from(exec_hint.as_int())
                    .map_err(NetworkAccountTargetError::DecodeExecutionHint)?;

                NetworkAccountTarget::new(target_id, exec_hint)
            },
            _ => Err(NetworkAccountTargetError::AttachmentKindMismatch(
                attachment.content().attachment_kind(),
            )),
        }
    }
}

// NETWORK ACCOUNT TARGET ERROR
// ================================================================================================

#[derive(Debug, thiserror::Error)]
pub enum NetworkAccountTargetError {
    #[error("target account ID must be of type network account")]
    TargetNotNetwork(AccountId),
    #[error(
        "attachment scheme {0} did not match expected type {expected}",
        expected = NetworkAccountTarget::ATTACHMENT_SCHEME
    )]
    AttachmentSchemeMismatch(NoteAttachmentScheme),
    #[error(
        "attachment kind {0} did not match expected type {expected}",
        expected = NoteAttachmentKind::Word
    )]
    AttachmentKindMismatch(NoteAttachmentKind),
    #[error("failed to decode target account ID")]
    DecodeTargetId(#[source] AccountIdError),
    #[error("failed to decode execution hint")]
    DecodeExecutionHint(#[source] NoteError),
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use std::string::ToString;
    use std::sync::Arc;

    use assert_matches::assert_matches;
    use miden_processor::fast::{ExecutionOutput, FastProcessor};
    use miden_processor::{AdviceInputs, DefaultHost, ExecutionError, Program};
    use miden_protocol::account::AccountStorageMode;
    use miden_protocol::assembly::{Assembler, DefaultSourceManager};
    use miden_protocol::note::{NoteMetadata, NoteTag, NoteType};
    use miden_protocol::testing::account_id::AccountIdBuilder;
    use miden_protocol::vm::StackInputs;
    use miden_protocol::{CoreLibrary, Felt, ProtocolLib};

    use super::*;
    use crate::standards_lib::StandardsLib;

    #[test]
    fn network_account_target_serde() -> anyhow::Result<()> {
        let id = AccountIdBuilder::new()
            .storage_mode(AccountStorageMode::Network)
            .build_with_rng(&mut rand::rng());
        let network_account_target = NetworkAccountTarget::new(id, NoteExecutionHint::Always)?;
        assert_eq!(
            network_account_target,
            NetworkAccountTarget::try_from(&NoteAttachment::from(network_account_target))?
        );

        Ok(())
    }

    fn assemble_program(source: &str) -> anyhow::Result<Program> {
        let program = Assembler::new(Arc::new(DefaultSourceManager::default()))
            .with_dynamic_library(CoreLibrary::default())
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .with_dynamic_library(StandardsLib::default())
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .with_dynamic_library(ProtocolLib::default())
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .assemble_program(source)
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;

        Ok(program)
    }

    async fn execute_program_with_default_host(
        program: Program,
    ) -> Result<ExecutionOutput, ExecutionError> {
        let mut host = DefaultHost::default();

        let core_lib = CoreLibrary::default();
        host.load_library(core_lib.mast_forest()).unwrap();

        let standards_lib = StandardsLib::default();
        host.load_library(standards_lib.mast_forest()).unwrap();

        let protocol_lib = ProtocolLib::default();
        host.load_library(protocol_lib.mast_forest()).unwrap();

        let stack_inputs = StackInputs::new(vec![]).unwrap();
        let advice_inputs = AdviceInputs::default();

        let processor = FastProcessor::new_debug(stack_inputs.as_slice(), advice_inputs);
        processor.execute(&program, &mut host).await
    }

    #[tokio::test]
    async fn network_account_target_get_id() -> anyhow::Result<()> {
        let target_id = AccountIdBuilder::new()
            .storage_mode(AccountStorageMode::Network)
            .build_with_rng(&mut rand::rng());
        let exec_hint = NoteExecutionHint::Always;

        let attachment = NoteAttachment::from(NetworkAccountTarget::new(target_id, exec_hint)?);
        let metadata =
            NoteMetadata::new(target_id, NoteType::Public, NoteTag::with_account_target(target_id))
                .with_attachment(attachment.clone());
        let metadata_header = metadata.to_header_word();

        let source = format!(
            r#"
            use miden::standards::attachments::network_account_target
            use miden::protocol::note

            begin
                push.{attachment_word}
                push.{metadata_header}
                exec.note::extract_attachment_info_from_metadata
                # => [attachment_kind, attachment_scheme, NOTE_ATTACHMENT]
                exec.network_account_target::get_id
                # cleanup stack
                movup.2 drop movup.2 drop
            end
            "#,
            metadata_header = metadata_header,
            attachment_word = attachment.content().to_word(),
        );

        let program = assemble_program(&source)?;
        let exec_output = execute_program_with_default_host(program).await?;

        assert_eq!(exec_output.stack[0], target_id.prefix().as_felt());
        assert_eq!(exec_output.stack[1], target_id.suffix());

        Ok(())
    }

    #[tokio::test]
    async fn network_account_target_new_attachment() -> anyhow::Result<()> {
        let target_id = AccountIdBuilder::new()
            .storage_mode(AccountStorageMode::Network)
            .build_with_rng(&mut rand::rng());
        let exec_hint = NoteExecutionHint::Always;

        let attachment = NoteAttachment::from(NetworkAccountTarget::new(target_id, exec_hint)?);
        let attachment_word = attachment.content().to_word();
        let expected_attachment_kind = Felt::from(attachment.attachment_kind().as_u8());

        let source = format!(
            r#"
            use miden::standards::attachments::network_account_target

            begin
                push.{exec_hint}
                push.{target_id_suffix}
                push.{target_id_prefix}
                # => [target_id_prefix, target_id_suffix, exec_hint]
                exec.network_account_target::new
                # => [attachment_scheme, attachment_kind, ATTACHMENT, pad(16)]

                # cleanup stack
                swapdw dropw dropw
            end
            "#,
            target_id_prefix = target_id.prefix().as_felt(),
            target_id_suffix = target_id.suffix(),
            exec_hint = Felt::from(exec_hint),
        );

        let program = assemble_program(&source)?;
        let exec_output = execute_program_with_default_host(program).await?;

        assert_eq!(exec_output.stack[0], expected_attachment_kind);
        assert_eq!(
            exec_output.stack[1],
            Felt::from(NetworkAccountTarget::ATTACHMENT_SCHEME.as_u32())
        );

        // TODO check why the attachment word is in reverse order
        assert_eq!(exec_output.stack[2], attachment_word[3]);
        assert_eq!(exec_output.stack[3], attachment_word[2]);
        assert_eq!(exec_output.stack[4], attachment_word[1]);
        assert_eq!(exec_output.stack[5], attachment_word[0]);

        Ok(())
    }

    #[tokio::test]
    async fn network_account_target_attachment_round_trip() -> anyhow::Result<()> {
        let target_id = AccountIdBuilder::new()
            .storage_mode(AccountStorageMode::Network)
            .build_with_rng(&mut rand::rng());
        let exec_hint = NoteExecutionHint::Always;

        let source = format!(
            r#"
            use miden::standards::attachments::network_account_target

            begin
                push.{exec_hint}
                push.{target_id_suffix}
                push.{target_id_prefix}
                # => [target_id_prefix, target_id_suffix, exec_hint]
                exec.network_account_target::new
                # => [attachment_scheme, attachment_kind, ATTACHMENT]
                exec.network_account_target::get_id
                # => [target_id_prefix, target_id_suffix]
                movup.2 drop movup.2 drop
            end
            "#,
            target_id_prefix = target_id.prefix().as_felt(),
            target_id_suffix = target_id.suffix(),
            exec_hint = Felt::from(exec_hint),
        );

        let program = assemble_program(&source)?;
        let exec_output = execute_program_with_default_host(program).await?;

        assert_eq!(exec_output.stack[0], target_id.prefix().as_felt());
        assert_eq!(exec_output.stack[1], target_id.suffix());

        Ok(())
    }

    #[test]
    fn network_account_target_fails_on_private_network_target_account() -> anyhow::Result<()> {
        let id = AccountIdBuilder::new()
            .storage_mode(AccountStorageMode::Private)
            .build_with_rng(&mut rand::rng());
        let err = NetworkAccountTarget::new(id, NoteExecutionHint::Always).unwrap_err();

        assert_matches!(
            err,
            NetworkAccountTargetError::TargetNotNetwork(account_id) if account_id == id
        );

        Ok(())
    }
}
