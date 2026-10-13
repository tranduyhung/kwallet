use std::cmp::Reverse;

use kaspa_consensus_core::tx::TransactionOutpoint;

use crate::error::KwalletError;

/// A spendable (mature) UTXO: how much it holds and where it lives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coin {
    pub amount: u64,
    pub outpoint: TransactionOutpoint,
}

/// The coins to spend, and the change to send back to ourselves.
#[derive(Debug, PartialEq)]
pub struct Selection {
    pub coins: Vec<Coin>,
    /// `None` means no change output. Outputs must be non-zero, so this is never `Some(0)`.
    pub change: Option<u64>,
}

pub fn select_coins(coins: &[Coin], amount: u64, fee: u64) -> Result<Selection, KwalletError> {
    let needed = amount
        .checked_add(fee)
        .ok_or(KwalletError::AmountOverflow)?;

    // Largest first. Reverse flips the order, so the biggest amount sorts to the front.
    let mut sorted: Vec<&Coin> = coins.iter().collect();
    sorted.sort_by_key(|coin| Reverse(coin.amount));

    let mut selected = Vec::new();
    let mut total: u64 = 0;

    for coin in sorted {
        total = total
            .checked_add(coin.amount)
            .ok_or(KwalletError::BalanceOverflow)?;
        selected.push(*coin);

        if total >= needed {
            // Cannot underflow: we only get here when total >= needed.
            let change = total - needed;
            return Ok(Selection {
                coins: selected,
                change: (change > 0).then_some(change),
            });
        }
    }

    // The loop added every coin and still came up short.
    Err(KwalletError::InsufficientFunds {
        available: total,
        needed,
    })
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::Hash;

    use super::*;

    const KAS: u64 = 100_000_000;

    // Each coin gets a distinct fake transaction id, so we can tell them apart.
    fn coin(amount: u64, id: u64) -> Coin {
        Coin {
            amount,
            outpoint: TransactionOutpoint::new(Hash::from_u64_word(id), 0),
        }
    }

    #[test]
    fn exact_match_has_no_change() {
        let coins = [coin(6, 1)];

        let selection = select_coins(&coins, 5, 1).unwrap();

        assert_eq!(
            selection,
            Selection {
                coins: vec![coin(6, 1)],
                change: None
            }
        );
    }

    #[test]
    fn one_coin_with_change() {
        let coins = [coin(10 * KAS, 1)];

        let selection = select_coins(&coins, 3 * KAS, 1_000).unwrap();

        assert_eq!(selection.coins, vec![coin(10 * KAS, 1)]);
        assert_eq!(selection.change, Some(7 * KAS - 1_000));
    }

    #[test]
    fn several_coins_needed() {
        let coins = [coin(3, 1), coin(4, 2), coin(5, 3)];

        // Needs 11: 5 + 4 = 9 is not enough, 5 + 4 + 3 = 12 is.
        let selection = select_coins(&coins, 10, 1).unwrap();

        assert_eq!(selection.coins, vec![coin(5, 3), coin(4, 2), coin(3, 1)]);
        assert_eq!(selection.change, Some(1));
    }

    #[test]
    fn insufficient_funds() {
        let coins = [coin(3, 1), coin(4, 2)];

        assert!(matches!(
            select_coins(&coins, 10, 1),
            Err(KwalletError::InsufficientFunds {
                available: 7,
                needed: 11
            })
        ));
    }

    // The input is unsorted, Largest-first must pick the single 5 instead of 1 + 3.
    #[test]
    fn largest_coin_is_picked_first() {
        let coins = [coin(1, 1), coin(5, 2), coin(3, 3)];

        let selection = select_coins(&coins, 4, 0).unwrap();

        assert_eq!(selection.coins, vec![coin(5, 2)]);
        assert_eq!(selection.change, Some(1));
    }

    #[test]
    fn amount_plus_fee_overflow() {
        let coins = [coin(1, 1)];

        assert!(matches!(
            select_coins(&coins, u64::MAX, 1),
            Err(KwalletError::AmountOverflow)
        ));
    }
}
