mod amount;
mod error;
mod keys;
mod network;
mod node;

use amount::{format_sompi, parse_send_amount};
use error::KwalletError;
use node::{DEFAULT_NODE_URL, connect_verified};

use clap::{Parser, Subcommand};
use kaspa_wrpc_client::prelude::GetServerInfoResponse;
use std::process::ExitCode;

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

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::New => {
            if let Err(e) = new_wallet() {
                eprintln!("Error occurred while creating new wallet: {e}");
                return ExitCode::FAILURE;
            }
        }
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

async fn info(url: &str) -> Result<(), KwalletError> {
    let (client, server_info) = connect_verified(url).await?;

    println!("connected to {url}");

    print_server_info(&server_info);

    client.disconnect().await?;

    println!("disconnected from {url}");

    Ok(())
}

fn new_wallet() -> Result<(), KwalletError> {
    let mnemonic = keys::generate_mnemonic()?;
    let master_key = keys::master_key(&mnemonic)?;
    let child_key = keys::first_receive_key(master_key)?;
    let address = keys::receive_address(&child_key)?;

    println!("DEVNET ONLY: never use this phrase for real funds.");
    println!("Write it down now; it will not be shown again.");
    println!();
    println!("{}", mnemonic.phrase());
    println!();
    println!("Address: {address}");
    println!();

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
}
