// Minimal address types that are used within miden-protocol
// The main Address type and related types are in miden-standards

mod network_id;
pub use network_id::{CustomNetworkId, NetworkId};

mod r#type;
pub use r#type::AddressType;
