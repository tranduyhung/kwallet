use std::{env, process, fmt};

const SOMPI_PER_KAS: u64 = 100_000_000;

// PartialEq and Debug to write assert_eq! in tests
#[derive(PartialEq, Debug)]
enum Command {
    New,
    Balance { address: Option<String> },
    Send { to: String, amount_sompi: u64 },
    Version,
    Help,
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

fn parse_kas(s: &str) -> Result<u64, AmountError> {
    // 1. Split "1.5" into {"1", "5"}; "1" into ["1"]
    let (whole_str, frac_str) = s.split_once('.').unwrap_or((s, ""));

    // 2. Only plain digits allowed, which rejects "-1", "+1", "1,5", "1.2.3", "abc"
    let is_digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());

    if !is_digits(whole_str) || !is_digits(frac_str) {
        return Err(AmountError::InvalidCharacters);
    }

    if whole_str.is_empty() && frac_str.is_empty() {
        return Err(AmountError::Empty);
    }

    // 3. Sompi is the smallest unit, so at most 8 decimals
    if frac_str.len() > 8 {
        return Err(AmountError::TooManyDecimals);
    }

    // 4. Parse each part as an integer; an empty whole part (".5") counts as 0
    let whole: u64 = if whole_str.is_empty() {
        0
    } else {
        whole_str.parse().map_err(|_| AmountError::TooLarge)?
    };

    // Pad on the right: "5" -> "50000000" (8 digits), "" -> "00000000"
    let frac: u64 = format!("{frac_str:0<8}").parse().map_err(|_| AmountError::InvalidCharacters)?;

    // 5. whole * 100_000_000 + frac, failing instead of overflowing
    whole
        .checked_mul(SOMPI_PER_KAS) // Returns an Option<u64>, Some if no overflow, None if overflow
        .and_then(|w| w.checked_add(frac)) // If this is Some, run the next checked_add, otherwise return None
        .ok_or(AmountError::TooLarge) // Convert Option into a Result
}

fn format_sompi(s: u64) -> String {
    format!("{}.{:08}", s / SOMPI_PER_KAS, s % SOMPI_PER_KAS)
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        [] | ["help"] => Ok(Command::Help),
        ["help", ..] => Err("usage: kwallet help".into()),
        ["version"] => Ok(Command::Version),
        ["version", ..] => Err("usage: kwallet version".into()),
        ["new"] => Ok(Command::New),
        ["new", ..] => Err("usage: kwallet new".into()),
        ["balance"] => Ok(Command::Balance { address: None }),
        ["balance", addr] => Ok(Command::Balance { address: Some(addr.to_string()) }),
        ["balance", ..] => Err("usage: kwallet balance [ADDRESS]".into()),
        ["send", to, amount] => {
            let amount_sompi = parse_kas(amount).map_err(|e| e.to_string())?;

            if amount_sompi == 0 {
                return Err(AmountError::Zero.to_string());
            }

            Ok(Command::Send { to: to.to_string(), amount_sompi })
        }
        ["send", ..] => Err("usage: kwallet send <ADDRESS> <AMOUNT_KAS>".into()),
        [cmd, ..] => Err(format!("unrecognized command '{cmd}'")),
    }
}

const HELP_TEXT: &str = concat!(
    "Usage: kwallet <COMMAND>\n\n",
    "Commands:\n",
    "    new                          New\n",
    "    balance [ADDRESS]            Balance\n",
    "    send <ADDRESS> <AMOUNT_KAS>  Send\n",
    "    version                      Print version\n",
    "    help                         Print help",
);

fn main() {
    // Collect command line arguments into a Vec<String>
    let args: Vec<String> = env::args().skip(1).collect();

    let command = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}\n\n{HELP_TEXT}");
            process::exit(1);
        }
    };

    match command {
        Command::Help => println!("{HELP_TEXT}"),
        Command::Version => println!("kwallet v{}", env!("CARGO_PKG_VERSION")),
        Command::New => println!("not implemented yet"),
        Command::Balance { address } => match address {
            Some(addr) => println!("balance for {addr}: not implemented yet"),
            None => println!("balance for all wallet addresses: not implemented yet"),
        },
        Command::Send { to, amount_sompi } => println!("send {} to {}: not implemented yet", format_sompi(amount_sompi), to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_kas_to_sompi() {
        assert_eq!(parse_kas("1"), Ok(100_000_000));
        assert_eq!(parse_kas("1.5"), Ok(150_000_000));
        assert_eq!(parse_kas("0.00000001"), Ok(1));
        assert_eq!(parse_kas("0.1"), Ok(10_000_000));
    }

    #[test]
    fn rejects_bad_amounts() {
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
    fn parses_zero() {
        assert_eq!(parse_kas("0"), Ok(0));
        assert_eq!(parse_kas("0.00000000"), Ok(0));
    }

    #[test]
    fn send_rejects_zero_amount() {
        for zero in ["0", "0.0", "0.00000000", ".0"] {
            let args = ["send", "addr", zero].map(String::from);
            assert_eq!(parse_args(&args), Err(AmountError::Zero.to_string()));
        }
    }

    #[test]
    fn parse_u64_max_boundary() {
        // u64::MAX = 18,446,744,073,709,551,615 sompi. Dividing by 10^8 gives 184467440737.09551615 KAS.
        // 184467440737 × 100_000_000 = 18,446,744,073,700,000,000, which still fits in a u64.
        // Adding 9551615 gives exactly u64::MAX, so the result is Some, then Ok.
        // Adding 9551616 is one too many. checked_add returns None, which becomes Err.
        assert_eq!(parse_kas("184467440737.09551615"), Ok(u64::MAX));
        assert_eq!(parse_kas("184467440737.09551616"), Err(AmountError::TooLarge));
    }

    #[test]
    fn parses_valid_commands() {
        assert_eq!(parse_args(&args(&[])), Ok(Command::Help));
        assert_eq!(parse_args(&args(&["help"])), Ok(Command::Help));
        assert_eq!(parse_args(&args(&["version"])), Ok(Command::Version));
        assert_eq!(parse_args(&args(&["new"])), Ok(Command::New));
        assert_eq!(parse_args(&args(&["balance"])), Ok(Command::Balance { address: None }));
        assert_eq!(
            parse_args(&args(&["balance", "addr"])),
            Ok(Command::Balance { address: Some("addr".to_string()) })
        );
        assert_eq!(
            parse_args(&args(&["send", "addr", "1.5"])),
            Ok(Command::Send { to: "addr".to_string(), amount_sompi: 150_000_000 })
        );
    }

    #[test]
    fn rejects_wrong_argument_counts() {
        assert_eq!(parse_args(&args(&["help", "x"])), Err("usage: kwallet help".to_string()));
        assert_eq!(parse_args(&args(&["version", "x"])), Err("usage: kwallet version".to_string()));
        assert_eq!(parse_args(&args(&["new", "x"])), Err("usage: kwallet new".to_string()));
        assert_eq!(parse_args(&args(&["balance", "a", "b"])), Err("usage: kwallet balance [ADDRESS]".to_string()));
        assert_eq!(parse_args(&args(&["send"])), Err("usage: kwallet send <ADDRESS> <AMOUNT_KAS>".to_string()));
        assert_eq!(parse_args(&args(&["send", "addr"])), Err("usage: kwallet send <ADDRESS> <AMOUNT_KAS>".to_string()));
    }

    #[test]
    fn rejects_unknown_command_and_bad_amount() {
        assert_eq!(parse_args(&args(&["bogus"])), Err("unrecognized command 'bogus'".to_string()));
        assert_eq!(parse_args(&args(&["send", "addr", "abc"])), Err(AmountError::InvalidCharacters.to_string()));
    }

}