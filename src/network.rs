use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};

/// The only network kwallet will talk to or make addresses for.
pub const ALLOWED_NETWORK: NetworkId = NetworkId::new(NetworkType::Devnet);
