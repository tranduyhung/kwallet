use clap::{Parser, Subcommand};
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType, RpcApi, RpcError};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
    prelude::GetServerInfoResponse,
};
use std::process::ExitCode;
use std::{error::Error, fmt, time::Duration};

const SOMPI_PER_KAS: u64 = 100_000_000;
const DEFAULT_NODE_URL: &str = "ws://127.0.0.1:17610";
const ALLOWED_NETWORK: NetworkId = NetworkId::new(NetworkType::Devnet);
const CONNECT_TIMEOUT_MS: u64 = 5_000;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)] // Copy the top-level --version flag (and its value) onto every subcommand.
#[command(arg_required_else_help = true)] // Print help when run with no subcommand.
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, PartialEq, Debug)]
enum Command {
    /// Create a new wallet
    New,
    /// Get the balance of a wallet address, or all addresses if none is specified
    Balance {
        /// Wallet address
        address: Option<String>,
    },
    /// Send KAS from the wallet to an address
    Send {
        /// Address to send to
        #[arg(value_name = "ADDRESS")]
        to: String,
        /// Amount in KAS, up to 8 decimals (e.g. 1.5)
        #[arg(value_name = "AMOUNT_KAS", value_parser = parse_send_amount)]
        amount_sompi: u64,
    },
    /// Print node info
    Info {
        /// URL of the KAS node to connect to
        #[arg(long, value_name = "URL", default_value = DEFAULT_NODE_URL)]
        url: String,
    },
}

#[derive(PartialEq, Debug)]
enum AmountError {
    Empty,
    InvalidCharacters,
    TooManyDecimals,
    TooLarge,
    Zero,
}

impl fmt::Display for AmountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            AmountError::Empty => "amount is empty",
            AmountError::InvalidCharacters => "invalid amount",
            AmountError::TooManyDecimals => "too many decimal places (max 8)",
            AmountError::TooLarge => "amount too large",
            AmountError::Zero => "amount must be greater than 0",
        };

        write!(f, "{msg}")
    }
}

impl Error for AmountError {}

#[derive(Debug)]
enum KwalletError {
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

fn parse_send_amount(s: &str) -> Result<u64, AmountError> {
    match parse_kas(s)? {
        0 => Err(AmountError::Zero),
        n => Ok(n),
    }
}

fn parse_kas(s: &str) -> Result<u64, AmountError> {
    // 1. Split "1.5" into {"1", "5"}; "1" into ["1"].
    let (whole_str, frac_str) = s.split_once('.').unwrap_or((s, ""));

    // 2. Only plain digits allowed, which rejects "-1", "+1", "1,5", "1.2.3", "abc".
    let is_digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());

    if !is_digits(whole_str) || !is_digits(frac_str) {
        return Err(AmountError::InvalidCharacters);
    }

    if whole_str.is_empty() && frac_str.is_empty() {
        return Err(AmountError::Empty);
    }

    // 3. Sompi is the smallest unit, so at most 8 decimals.
    if frac_str.len() > 8 {
        return Err(AmountError::TooManyDecimals);
    }

    // 4. Parse each part as an integer; an empty whole part (".5") counts as 0.
    let whole: u64 = if whole_str.is_empty() {
        0
    } else {
        whole_str.parse().map_err(|_| AmountError::TooLarge)?
    };

    // Pad on the right: "5" -> "50000000" (8 digits), "" -> "00000000".
    // The map_err never fires: frac_str is at most 8 ASCII digits, which always fits in a u64.
    // Alternative:
    // .parse().expect("frac_str is at most 8 ASCII digits, so it always fits in a u64");
    let frac: u64 = format!("{frac_str:0<8}")
        .parse()
        .map_err(|_| AmountError::InvalidCharacters)?;

    // 5. whole * 100_000_000 + frac, failing instead of overflowing.
    whole
        .checked_mul(SOMPI_PER_KAS) // Returns an Option<u64>, Some if no overflow, None if overflow.
        .and_then(|w| w.checked_add(frac)) // If this is Some, run the next checked_add, otherwise return None.
        .ok_or(AmountError::TooLarge) // Convert Option into a Result.
}

fn format_sompi(s: u64) -> String {
    format!("{}.{:08}", s / SOMPI_PER_KAS, s % SOMPI_PER_KAS)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::New => println!("not implemented yet"),
        Command::Balance { address } => match address {
            Some(addr) => println!("balance for {addr}: not implemented yet"),
            None => println!("balance for all wallet addresses: not implemented yet"),
        },
        Command::Send { to, amount_sompi } => println!(
            "send {} to {}: not implemented yet",
            format_sompi(amount_sompi),
            to
        ),
        Command::Info { url } => {
            if let Err(e) = info(&url).await {
                eprintln!("Error occurred while fetching info from {url}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

/// Connects to the node and refuses to continue unless it is on `ALLOWED_NETWORK`.
/// Every command that talks to a node must get its client from here.
async fn connect_verified(
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

fn print_server_info(info: &GetServerInfoResponse) {
    let GetServerInfoResponse {
        is_synced,
        server_version,
        network_id,
        virtual_daa_score,
        ..
    } = info;

    println!("Node version: {server_version}");
    println!("Network: {network_id}");
    println!("Node is synced: {is_synced}");
    println!("Virtual DAA score: {virtual_daa_score}");
}

async fn fetch_verified_info(
    client: &KaspaRpcClient,
) -> Result<GetServerInfoResponse, KwalletError> {
    let info = client.get_server_info().await?;

    check_network(info.network_id)?;

    Ok(info)
}

async fn info(url: &str) -> Result<(), KwalletError> {
    let (client, server_info) = connect_verified(url).await?;

    println!("connected to {url}");

    print_server_info(&server_info);

    client.disconnect().await?;

    println!("disconnected from {url}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_kas_to_sompi() {
        assert_eq!(parse_kas("1"), Ok(100_000_000));
        assert_eq!(parse_kas("1.5"), Ok(150_000_000));
        assert_eq!(parse_kas("0.00000001"), Ok(1));
        assert_eq!(parse_kas("0.1"), Ok(10_000_000));
    }

    #[test]
    fn reject_invalid_commands() {
        let cases: [(&[&str], ErrorKind); 5] = [
            (
                &["kwallet", "send", "address"],
                ErrorKind::MissingRequiredArgument,
            ),
            (&["kwallet", "new", "extra"], ErrorKind::UnknownArgument),
            (&["kwallet", "bogus"], ErrorKind::InvalidSubcommand),
            (
                &["kwallet", "send", "addr", "0"],
                ErrorKind::ValueValidation,
            ),
            (
                &["kwallet", "send", "addr", "abc"],
                ErrorKind::ValueValidation,
            ),
        ];

        for (args, expected) in cases {
            let err = Cli::try_parse_from(args).unwrap_err();
            assert_eq!(err.kind(), expected, "args {args:?}");
        }
    }

    #[test]
    fn reject_bad_amounts() {
        let cases = [
            ("", AmountError::Empty),
            (".", AmountError::Empty),
            ("abc", AmountError::InvalidCharacters),
            ("-1", AmountError::InvalidCharacters),
            ("+1", AmountError::InvalidCharacters),
            ("1.2.3", AmountError::InvalidCharacters),
            ("1,5", AmountError::InvalidCharacters),
            ("0.000000001", AmountError::TooManyDecimals),
            ("999999999999.99999999", AmountError::TooLarge),
            ("99999999999999999999999", AmountError::TooLarge),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_kas(input), Err(expected), "input {input:?}");
        }
    }

    #[test]
    fn parse_zero() {
        assert_eq!(parse_kas("0"), Ok(0));
        assert_eq!(parse_kas("0.00000000"), Ok(0));
    }

    #[test]
    fn send_rejects_zero_amount() {
        for zero in ["0", "0.0", "0.00000000", ".0"] {
            assert_eq!(
                parse_send_amount(zero),
                Err(AmountError::Zero),
                "input {zero:?}"
            );
        }
    }

    #[test]
    fn parse_u64_max_boundary() {
        // u64::MAX = 18,446,744,073,709,551,615 sompi. Dividing by 10^8 gives 184467440737.09551615 KAS.
        // 184467440737 × 100_000_000 = 18,446,744,073,700,000,000, which still fits in a u64.
        // Adding 9551615 gives exactly u64::MAX, so the result is Some, then Ok.
        // Adding 9551616 is one too many. checked_add returns None, which becomes Err.
        assert_eq!(parse_kas("184467440737.09551615"), Ok(u64::MAX));
        assert_eq!(
            parse_kas("184467440737.09551616"),
            Err(AmountError::TooLarge)
        );
    }

    #[test]
    fn parse_valid_commands() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;

        assert_eq!(parse(&["kwallet", "new"]), Command::New);
        assert_eq!(
            parse(&["kwallet", "balance"]),
            Command::Balance { address: None }
        );
        assert_eq!(
            parse(&["kwallet", "balance", "addr"]),
            Command::Balance {
                address: Some("addr".to_string())
            }
        );
        assert_eq!(
            parse(&["kwallet", "send", "addr", "1.5"]),
            Command::Send {
                to: "addr".to_string(),
                amount_sompi: 150_000_000
            }
        );
        assert_eq!(
            parse(&["kwallet", "info"]),
            Command::Info {
                url: DEFAULT_NODE_URL.to_string()
            }
        )
    }

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
