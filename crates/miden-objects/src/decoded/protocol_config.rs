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
