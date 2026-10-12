use kaspa_wrpc_client::prelude::RpcUtxoEntry;

use crate::error::KwalletError;

/// How much of a balance can be spent now, and how much is still waiting for coinbase maturity.
#[derive(Debug, PartialEq)]
pub struct Balance {
    pub mature: u64,
    pub immature: u64,
}

impl Balance {
    /// Mature plus immature, failing instead of overflowing.
    pub fn total(&self) -> Result<u64, KwalletError> {
        self.mature
            .checked_add(self.immature)
            .ok_or(KwalletError::BalanceOverflow)
    }
}

// Adds up all the amounts. Returns an error if the total is too big for a u64.
// We don't trust the node, so we check every addition.
// A plain .sum() would crash on overflow in debug builds and give a wrong number in release builds.
fn checked_sum(mut amounts: impl Iterator<Item = u64>) -> Result<u64, KwalletError> {
    amounts
        .try_fold(0u64, |total, amount| total.checked_add(amount)) // Start at 0 and add each amount. Stop at the first overflow.
        .ok_or(KwalletError::BalanceOverflow) // Turn the Option into a Result.
}

// Only coinbase (mining reward) outputs need to wait. Normal outputs can be spent right away.
// The UTXOs are fetched after virtual_daa_score, so new blocks can arrive in between.
// A fresh coinbase can then have a block_daa_score higher than ours. saturating_sub gives 0, so it counts as immature.
fn is_mature(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    !entry.is_coinbase
        || virtual_daa_score.saturating_sub(entry.block_daa_score) >= coinbase_maturity
}

pub fn split_by_maturity(
    entries: &[RpcUtxoEntry],
    virtual_daa_score: u64,
    coinbase_maturity: u64,
) -> Result<Balance, KwalletError> {
    // One pass: each entry goes into exactly one of the two lists.
    let (mature, immature): (Vec<&RpcUtxoEntry>, Vec<&RpcUtxoEntry>) = entries
        .iter()
        .partition(|entry| is_mature(entry, virtual_daa_score, coinbase_maturity));

    let mature = checked_sum(mature.iter().map(|entry| entry.amount))?;
    let immature = checked_sum(immature.iter().map(|entry| entry.amount))?;

    Ok(Balance { mature, immature })
}

#[cfg(test)]
mod tests {
    use kaspa_wrpc_client::prelude::{RpcScriptPublicKey, RpcUtxoEntry};

    use super::*;

    const MATURITY: u64 = 1_000;

    // Makes a test entry. Only the amount matters here; the other fields are empty.
    fn entry(amount: u64) -> RpcUtxoEntry {
        RpcUtxoEntry::new(amount, RpcScriptPublicKey::default(), 0, false, None)
    }

    // Make a coinbase (mining reward) entry from the block with this DAA score.
    fn coinbase(amount: u64, block_daa_score: u64) -> RpcUtxoEntry {
        RpcUtxoEntry::new(
            amount,
            RpcScriptPublicKey::default(),
            block_daa_score,
            true,
            None,
        )
    }

    #[test]
    fn no_entries_is_zero() {
        let balance = split_by_maturity(&[], 0, MATURITY).unwrap();

        assert_eq!(balance.total().unwrap(), 0);
    }

    #[test]
    fn total_adds_all_amounts() {
        let entries = [entry(100_000_000), entry(50_000_000), entry(1)];

        let balance = split_by_maturity(&entries, 0, MATURITY).unwrap();

        assert_eq!(balance.total().unwrap(), 150_000_001);
    }

    // A total of exactly u64::MAX still fits, so this must work.
    #[test]
    fn total_reaches_u64_max() {
        let balance = Balance {
            mature: u64::MAX - 1,
            immature: 1,
        };

        assert_eq!(balance.total().unwrap(), u64::MAX);
    }

    // Each group fits on its own, but together they are too big.
    #[test]
    fn total_rejects_overflow() {
        let balance = Balance {
            mature: u64::MAX / 2 + 1,
            immature: u64::MAX / 2 + 1,
        };

        assert!(matches!(
            balance.total(),
            Err(KwalletError::BalanceOverflow)
        ));
    }

    // Each amount is a bit more than half of u64::MAX, so one group alone is too big.
    #[test]
    fn split_rejects_overflow_in_one_group() {
        let entries = [entry(u64::MAX / 2 + 1), entry(u64::MAX / 2 + 1)];

        assert!(matches!(
            split_by_maturity(&entries, 0, MATURITY),
            Err(KwalletError::BalanceOverflow)
        ));
    }

    // A normal output from a very recent block can still be spent right away.
    #[test]
    fn normal_output_is_always_mature() {
        let entries = [RpcUtxoEntry::new(
            5,
            RpcScriptPublicKey::default(),
            100,
            false,
            None,
        )];

        let balance = split_by_maturity(&entries, 100, MATURITY).unwrap();

        assert_eq!(
            balance,
            Balance {
                mature: 5,
                immature: 0
            }
        );
    }

    // Exactly MATURITY scores later, the coinbase becomes spendable.
    #[test]
    fn coinbase_is_mature_at_the_boundary() {
        let entries = [coinbase(5, 100)];

        let balance = split_by_maturity(&entries, 100 + MATURITY, MATURITY).unwrap();

        assert_eq!(
            balance,
            Balance {
                mature: 5,
                immature: 0
            }
        );
    }

    // One score before the boundary, it still has to wait.
    #[test]
    fn coinbase_is_immature_just_before_the_boundary() {
        let entries = [coinbase(5, 100)];

        let balance = split_by_maturity(&entries, 100 + MATURITY - 1, MATURITY).unwrap();

        assert_eq!(
            balance,
            Balance {
                mature: 0,
                immature: 5
            }
        );
    }

    // The UTXOs are fetched a moment after virtual_daa_score, and on a 10 BPS network new blocks
    // arrive in between. A coinbase from one of those blocks must count as immature, not underflow.
    #[test]
    fn coinbase_newer_than_our_daa_score_is_immature() {
        let entries = [coinbase(5, 2_000)];

        let balance = split_by_maturity(&entries, 1_000, MATURITY).unwrap();

        assert_eq!(
            balance,
            Balance {
                mature: 0,
                immature: 5
            }
        );
    }

    // Every entry lands in exactly one of the two groups.
    #[test]
    fn mature_plus_immature_is_total() {
        let entries = [entry(1), coinbase(20, 0), coinbase(300, 4_500)];

        let balance = split_by_maturity(&entries, 5_000, MATURITY).unwrap();

        assert_eq!(
            balance,
            Balance {
                mature: 21,
                immature: 300
            }
        );
        assert_eq!(balance.total().unwrap(), 321);
    }
}
