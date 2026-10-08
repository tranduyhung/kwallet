use std::{error::Error, fmt};

const SOMPI_PER_KAS: u64 = 100_000_000;

#[derive(PartialEq, Debug)]
pub enum AmountError {
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

pub fn parse_send_amount(s: &str) -> Result<u64, AmountError> {
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

pub fn format_sompi(s: u64) -> String {
    format!("{}.{:08}", s / SOMPI_PER_KAS, s % SOMPI_PER_KAS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kas_to_sompi() {
        assert_eq!(parse_kas("1"), Ok(100_000_000));
        assert_eq!(parse_kas("1.5"), Ok(150_000_000));
        assert_eq!(parse_kas("0.00000001"), Ok(1));
        assert_eq!(parse_kas("0.1"), Ok(10_000_000));
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
}
