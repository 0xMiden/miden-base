//! Domain construction for decoded protocol_config messages.
pub use proto::protocol_config::DecodedKernelConfig as KernelConfig;

use crate::{Verify, proto};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_utils;

impl Verify for KernelConfig {
    type Verified = miden_protocol::protocol_config::KernelConfig;
    type Error = miden_protocol::errors::ProtocolConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::new(self.main_proc, self.kernel_procs)
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
    #[error("invalid fee asset ID: {0}")]
    FeeAssetId(#[from] miden_protocol::errors::AssetError),
    #[error("{0}")]
    Config(#[from] miden_protocol::errors::ProtocolConfigError),
    #[error("minimum security bits do not fit in a u8: {0}")]
    MinimumBits(#[from] core::num::TryFromIntError),
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

pub use proto::protocol_config::DecodedProtocolConfig as ProtocolConfig;

impl Verify for ProtocolConfig {
    type Verified = miden_protocol::protocol_config::ProtocolConfig;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            miden_protocol::asset::AssetId::try_from(self.fee_asset_id)?,
            self.tx_kernel.verify()?,
            self.batch_kernel.verify()?,
            self.block_kernel.verify()?,
            self.proof_verification.verify()?,
        )?)
    }
}
