mod amount;
mod balance;
mod error;
mod keys;
mod network;
mod node;

use amount::{format_sompi, parse_send_amount};
use error::KwalletError;
use node::{DEFAULT_NODE_URL, connect_verified};

use clap::{Parser, Subcommand};
use kaspa_wrpc_client::prelude::{GetServerInfoResponse, RpcApi, RpcUtxoEntry};
use std::process::ExitCode;

use crate::balance::{Balance, split_by_maturity, total_sompi};
use crate::network::{coinbase_maturity, parse_address};
use crate::node::disconnect_or_warn;

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
    /// Get the balance of a wallet address
    Balance {
        /// Wallet address
        address: String,
        /// URL of the KAS node to connect to
        #[arg(long, value_name = "URL", default_value = DEFAULT_NODE_URL)]
        url: String,
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
        Command::Balance { url, address } => {
            if let Err(e) = balance(&url, &address).await {
                eprintln!("Error occured while fetching balance for {address}: {e}");
                return ExitCode::FAILURE;
            }
        }
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

    disconnect_or_warn(&client, url).await;

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

async fn balance(url: &str, address: &str) -> Result<(), KwalletError> {
    // Check the address first, so a wrong-network address never touches the network.
    let parsed_address = parse_address(address)?;

    let (client, server_info) = connect_verified(url).await?;

    // No "?" here, we must disconnect even if the fetch fails.
    let fetched = client
        .get_utxos_by_addresses(vec![parsed_address.clone()])
        .await;

    disconnect_or_warn(&client, url).await;

    // The node wraps each UTXO with its address and outpoint; we only need the inner entry.
    let entries: Vec<RpcUtxoEntry> = fetched?.into_iter().map(|entry| entry.utxo_entry).collect();

    let total = total_sompi(&entries)?;
    let Balance { mature, immature } =
        split_by_maturity(&entries, server_info.virtual_daa_score, coinbase_maturity())?;

    println!("Address: {parsed_address}");
    println!(
        "Balance: {} KAS ({} UTXOs)",
        format_sompi(total),
        entries.len()
    );
    println!("  Mature:   {} KAS", format_sompi(mature));
    println!("  Immature: {} KAS", format_sompi(immature));

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
        let cases: [(&[&str], ErrorKind); 6] = [
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
            (&["kwallet", "balance"], ErrorKind::MissingRequiredArgument),
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
            parse(&["kwallet", "balance", "addr"]),
            Command::Balance {
                address: "addr".to_string(),
                url: DEFAULT_NODE_URL.to_string()
            }
        );
        assert_eq!(
            parse(&["kwallet", "balance", "addr", "--url", "ws://node:1234"]),
            Command::Balance {
                address: "addr".to_string(),
                url: "ws://node:1234".to_string()
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
