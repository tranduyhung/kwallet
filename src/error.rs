use kaspa_wrpc_client::prelude::{NetworkId, RpcError};
use std::{error::Error, fmt};

#[derive(Debug)]
pub enum KwalletError {
    Rpc(Box<kaspa_wrpc_client::error::Error>),
    WrongNetwork {
        expected: NetworkId,
        actual: NetworkId,
    },
}

impl fmt::Display for KwalletError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KwalletError::Rpc(e) => write!(f, "RPC error: {e}"),
            KwalletError::WrongNetwork { expected, actual } => write!(
                f,
                "refusing to continue: node is on {actual}, kwallet only supports {expected}"
            ),
        }
    }
}

impl Error for KwalletError {}

impl From<kaspa_wrpc_client::error::Error> for KwalletError {
    fn from(e: kaspa_wrpc_client::error::Error) -> Self {
        KwalletError::Rpc(Box::new(e))
    }
}

impl From<RpcError> for KwalletError {
    fn from(e: RpcError) -> Self {
        KwalletError::Rpc(Box::new(kaspa_wrpc_client::error::Error::from(e)))
    }
}
