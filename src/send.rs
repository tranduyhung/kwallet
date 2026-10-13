use std::cmp::Reverse;

use kaspa_addresses::Address;
use kaspa_consensus_core::{
    constants::TX_VERSION,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
        UtxoEntry,
    },
};
use kaspa_txscript::pay_to_address_script;

use crate::error::KwalletError;

/// A spendable (mature) UTXO: how much it holds and where it lives.
#[derive(Debug, Clone, PartialEq)]
pub struct Coin {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
}

/// The coins to spend, and the change to send back to ourselves.
#[derive(Debug, PartialEq)]
pub struct Selection {
    pub coins: Vec<Coin>,
    /// What the recipient gets, in sompi.
    pub amount: u64,
    /// What the miner gets, in sompi.
    pub fee: u64,
    /// `None` means no change output. Outputs must be non-zero, so this is never `Some(0)`.
    pub change: Option<u64>,
}

/// Pick coins largest-first until they cover `amount + fee`, and work out the change.
///
/// `amount` must be greater than 0. The CLI guarantees this through `parse_send_amount`.
pub fn select_coins(coins: &[Coin], amount: u64, fee: u64) -> Result<Selection, KwalletError> {
    let needed = amount
        .checked_add(fee)
        .ok_or(KwalletError::AmountOverflow)?;

    // Largest first. Reverse flips the order, so the biggest amount sorts to the front.
    let mut sorted: Vec<&Coin> = coins.iter().collect();
    sorted.sort_by_key(|coin| Reverse(coin.entry.amount));

    let mut selected = Vec::new();
    let mut total: u64 = 0;

    for coin in sorted {
        total = total
            .checked_add(coin.entry.amount)
            .ok_or(KwalletError::BalanceOverflow)?;
        selected.push(coin.clone());

        if total >= needed {
            // Cannot underflow: we only get here when total >= needed.
            let change = total - needed;
            return Ok(Selection {
                coins: selected,
                amount,
                fee,
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

/// Build the unsigned transaction for a selection. The signature scripts stay empty until signing.
pub fn build_transaction(
    selection: &Selection,
    recipient: &Address,
    change_address: &Address,
) -> SignableTransaction {
    let inputs: Vec<TransactionInput> = selection
        .coins
        .iter()
        .map(|coin| TransactionInput::new(coin.outpoint, vec![], 0, 1))
        .collect();

    let mut outputs = vec![TransactionOutput::new(
        selection.amount,
        pay_to_address_script(recipient),
    )];
    if let Some(change) = selection.change {
        outputs.push(TransactionOutput::new(
            change,
            pay_to_address_script(change_address),
        ));
    }

    let tx = Transaction::new(
        TX_VERSION,
        inputs,
        outputs,
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );

    // The signer needs each input's UTXO entry, in the same order as the inputs.
    let entries = selection
        .coins
        .iter()
        .map(|coin| coin.entry.clone())
        .collect();
    SignableTransaction::with_entries(tx, entries)
}

#[cfg(test)]
mod tests {
    use kaspa_addresses::{Prefix, Version};
    use kaspa_consensus_core::{Hash, tx::ScriptPublicKey};

    use super::*;

    const KAS: u64 = 100_000_000;

    // Each coin gets a distinct fake transaction id, so we can tell them apart.
    fn coin(amount: u64, id: u64) -> Coin {
        Coin {
            outpoint: TransactionOutpoint::new(Hash::from_u64_word(id), 0),
            entry: UtxoEntry::new(amount, ScriptPublicKey::default(), 0, false, None),
        }
    }

    fn address(byte: u8) -> Address {
        Address::new(Prefix::Devnet, Version::PubKey, &[byte; 32])
    }

    #[test]
    fn exact_match_has_no_change() {
        let coins = [coin(6, 1)];

        let selection = select_coins(&coins, 5, 1).unwrap();

        assert_eq!(
            selection,
            Selection {
                coins: vec![coin(6, 1)],
                amount: 5,
                fee: 1,
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

    // Each coin fits, and so does amount + fee. Adding the second coin to the total overflows.
    #[test]
    fn total_overflow_while_selecting() {
        let coins = [coin(u64::MAX / 2 + 1, 1), coin(u64::MAX / 2 + 1, 2)];

        assert!(matches!(
            select_coins(&coins, u64::MAX, 0),
            Err(KwalletError::BalanceOverflow)
        ))
    }

    #[test]
    fn one_input_per_selected_coin() {
        let coins = [coin(3, 1), coin(4, 2), coin(5, 3)];
        let selection = select_coins(&coins, 10, 1).unwrap();

        let tx = build_transaction(&selection, &address(1), &address(2));

        assert_eq!(tx.tx.inputs.len(), selection.coins.len());
        assert_eq!(tx.entries.len(), selection.coins.len());
    }

    #[test]
    fn outputs_pay_recipient_then_change() {
        let coins = [coin(10 * KAS, 1)];
        let selection = select_coins(&coins, 3 * KAS, 1_000).unwrap();
        let recipient = address(1);
        let me = address(2);

        let tx = build_transaction(&selection, &recipient, &me);

        let outputs = &tx.tx.outputs;
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].value, 3 * KAS);
        assert_eq!(
            outputs[0].script_public_key,
            pay_to_address_script(&recipient)
        );
        assert_eq!(outputs[1].value, 7 * KAS - 1_000);
        assert_eq!(outputs[1].script_public_key, pay_to_address_script(&me));
    }

    #[test]
    fn no_change_output_without_change() {
        let coins = [coin(6, 1)];
        let selection = select_coins(&coins, 5, 1).unwrap();

        let tx = build_transaction(&selection, &address(1), &address(2));

        assert_eq!(tx.tx.outputs.len(), 1);
        assert_eq!(tx.tx.outputs[0].value, 5);
    }

    #[test]
    // Guards against dropping the change output, which would hand it to the miner as fee.

    fn inputs_minus_outputs_equals_fee() {
        let fee = 1_000;
        let coins = [coin(3 * KAS, 1), coin(4 * KAS, 2)];
        let selection = select_coins(&coins, 5 * KAS, fee).unwrap();

        let tx = build_transaction(&selection, &address(1), &address(2));

        let total_in: u64 = tx.entries.iter().map(|e| e.as_ref().unwrap().amount).sum();
        let total_out: u64 = tx.tx.outputs.iter().map(|o| o.value).sum();
        assert_eq!(total_in - total_out, fee);
    }
}
