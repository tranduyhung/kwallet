use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::config::params::Params;
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};

use crate::error::KwalletError;

/// The only network kwallet will talk to or make addresses for.
pub const ALLOWED_NETWORK: NetworkId = NetworkId::new(NetworkType::Devnet);

/// Parses an address and refuses it unless it belongs to `ALLOWED_NETWORK`.
pub fn parse_address(s: &str) -> Result<Address, KwalletError> {
    let address = Address::try_from(s)?;
    let expected = Prefix::from(ALLOWED_NETWORK);

    if address.prefix != expected {
        return Err(KwalletError::WrongAddressNetwork {
            expected,
            actual: address.prefix,
        });
    }

    Ok(address)
}

/// How many DAA scores a coinbase output must wait before it can be spent on ALLOWED_NETWORK.
pub fn coinbase_maturity() -> u64 {
    Params::from(ALLOWED_NETWORK).coinbase_maturity()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Devnet runs at 10 blocks per second and coinbase outputs wait 100 seconds: 10 * 100.
    #[test]
    fn devnet_coinbase_maturity() {
        assert_eq!(coinbase_maturity(), 1_000);
    }
}
