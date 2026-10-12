use kaspa_wrpc_client::prelude::RpcUtxoEntry;

use crate::error::KwalletError;

/// How much of a balance can be spent now, and how much is still waiting for coinbase maturity.
#[derive(Debug, PartialEq)]
pub struct Balance {
    pub mature: u64,
    pub immature: u64,
}

// Adds up all the amounts. Returns an error if the total is too big for a u64.
// We don't trust the node, so we check every addition.
// A plain .sum() would crash on overflow in debug builds and give a wrong number in release builds.
fn checked_sum(mut amounts: impl Iterator<Item = u64>) -> Result<u64, KwalletError> {
    amounts
        .try_fold(0u64, |total, amount| total.checked_add(amount)) // Start at 0 and add each amount. Stop at the first overflow.
        .ok_or(KwalletError::BalanceOverflow) // Turn the Option into a Result.
}

pub fn total_sompi(entries: &[RpcUtxoEntry]) -> Result<u64, KwalletError> {
    checked_sum(entries.iter().map(|entry| entry.amount))
}

// Only coinbase (mining reward) outputs need to wait. Normal outputs can be spent right away.
// A coinbase output is mature once `coinbase_maturity` DAA scores have passed since its block.
// saturating_sub gives 0 instead of underflowing if the node sends a block score from the future.
fn is_mature(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    !entry.is_coinbase
        || virtual_daa_score.saturating_sub(entry.block_daa_score) >= coinbase_maturity
}

pub fn split_by_maturity(
    entries: &[RpcUtxoEntry],
    virtual_daa_score: u64,
    coinbase_maturity: u64,
) -> Result<Balance, KwalletError> {
    let mature = checked_sum(
        entries
            .iter()
            .filter(|entry| is_mature(entry, virtual_daa_score, coinbase_maturity))
            .map(|entry| entry.amount),
    )?;

    let immature = checked_sum(
        entries
            .iter()
            .filter(|entry| !is_mature(entry, virtual_daa_score, coinbase_maturity))
            .map(|entry| entry.amount),
    )?;

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
    fn total_of_no_entries_is_zero() {
        assert!(matches!(total_sompi(&[]), Ok(0)))
    }

    #[test]
    fn total_adds_all_amounts() {
        let entries = [entry(100_000_000), entry(50_000_000), entry(1)];

        assert_eq!(total_sompi(&entries).unwrap(), 150_000_001);
    }

    // A total of exactly u64::MAX still fits, so this must work.
    #[test]
    fn total_reaches_u64_max() {
        let entries = [entry(u64::MAX - 1), entry(1)];

        assert_eq!(total_sompi(&entries).unwrap(), u64::MAX);
    }

    // Each amount is a bit more than half of u64::MAX, so the total is too big.
    #[test]
    fn total_rejects_overflow() {
        let entries = [entry(u64::MAX / 2 + 1), entry(u64::MAX / 2 + 1)];

        assert!(matches!(
            total_sompi(&entries),
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

    // A bad node could send a block score higher than the current one; that must not underflow.
    #[test]
    fn coinbase_from_the_future_is_immature() {
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
        assert_eq!(
            balance.mature + balance.immature,
            total_sompi(&entries).unwrap()
        );
    }
}
