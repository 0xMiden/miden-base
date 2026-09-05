//! Domain construction for decoded protocol_config messages.
pub use proto::protocol_config::DecodedKernelConfig as KernelConfig;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for KernelConfig {
    type Verified = miden_protocol::protocol_config::KernelConfig;
    type Error = miden_protocol::errors::ProtocolConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::new(self.main_proc, self.kernel_procs)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::protocol_config::KernelConfig>
    for miden_protocol::protocol_config::KernelConfig
{
    type Error = ConversionError;
    fn try_from(value: proto::protocol_config::KernelConfig) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::protocol_config::DecodedProofSecurityPolicy as ProofSecurityPolicy;

impl Verify for ProofSecurityPolicy {
    type Verified = miden_protocol::protocol_config::ProofSecurityPolicy;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.security_estimator_root,
            self.minimum_bits.try_into()?,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("{0}")]
    Config(#[from] miden_protocol::errors::ProtocolConfigError),
    #[error("minimum security bits do not fit in a u8: {0}")]
    MinimumBits(#[from] core::num::TryFromIntError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::protocol_config::ProofSecurityPolicy>
    for miden_protocol::protocol_config::ProofSecurityPolicy
{
    type Error = ConversionError;
    fn try_from(value: proto::protocol_config::ProofSecurityPolicy) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::protocol_config::DecodedProofVerificationConfig as ProofVerificationConfig;

impl Verify for ProofVerificationConfig {
    type Verified = miden_protocol::protocol_config::ProofVerificationConfig;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.vm_verifier_root,
            self.precompile_verifier_root,
            self.security_policy.verify()?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::protocol_config::ProofVerificationConfig>
    for miden_protocol::protocol_config::ProofVerificationConfig
{
    type Error = ConversionError;
    fn try_from(
        value: proto::protocol_config::ProofVerificationConfig,
    ) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}
