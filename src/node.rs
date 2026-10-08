use crate::error::KwalletError;
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType, RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
    prelude::GetServerInfoResponse,
};
use std::time::Duration;

pub const DEFAULT_NODE_URL: &str = "ws://127.0.0.1:17610";
const ALLOWED_NETWORK: NetworkId = NetworkId::new(NetworkType::Devnet);
const CONNECT_TIMEOUT_MS: u64 = 5_000;

fn check_network(actual: NetworkId) -> Result<(), KwalletError> {
    if actual == ALLOWED_NETWORK {
        Ok(())
    } else {
        Err(KwalletError::WrongNetwork {
            expected: ALLOWED_NETWORK,
            actual,
        })
    }
}

/// Connects to the node and refuses to continue unless it is on `ALLOWED_NETWORK`.
/// Every command that talks to a node must get its client from here.
pub async fn connect_verified(
    url: &str,
) -> Result<(KaspaRpcClient, GetServerInfoResponse), KwalletError> {
    let encoding = WrpcEncoding::Borsh;

    let resolver = None;

    let selected_network = None;

    let subscription_context = None;

    let client = KaspaRpcClient::new(
        encoding,
        Some(url),
        resolver,
        selected_network,
        subscription_context,
    )?;

    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(CONNECT_TIMEOUT_MS)),
        strategy: ConnectStrategy::Fallback, // Retry would hang forever on an unreachable node.
        ..Default::default()
    };

    client.connect(Some(options)).await?;

    match fetch_verified_info(&client).await {
        Ok(info) => Ok((client, info)),
        Err(e) => {
            if let Err(disconnect_err) = client.disconnect().await {
                eprintln!("warning: failed to disconnect from {url}: {disconnect_err}");
            }

            Err(e)
        }
    }
}

async fn fetch_verified_info(
    client: &KaspaRpcClient,
) -> Result<GetServerInfoResponse, KwalletError> {
    let info = client.get_server_info().await?;

    check_network(info.network_id)?;

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_wrong_network() {
        let networks = [
            NetworkId::new(NetworkType::Mainnet),
            NetworkId::with_suffix(NetworkType::Testnet, 10),
        ];

        for network in networks {
            assert!(matches!(
                check_network(network),
                Err(KwalletError::WrongNetwork { expected, actual })
                    if expected == ALLOWED_NETWORK && actual == network
            ))
        }
    }

    #[test]
    fn accept_allowed_network() {
        assert!(matches!(check_network(ALLOWED_NETWORK), Ok(())));
    }
}
