use std::cmp::Reverse;

use crate::keys::{first_receive_key, generate_mnemonic, master_key, receive_address};
use kaspa_addresses::Address;
use kaspa_bip32::{
    ExtendedPrivateKey, SecretKey,
    secp256k1::{Keypair, Secp256k1},
};
use kaspa_consensus_core::{
    Hash,
    config::params::Params,
    constants::TX_VERSION,
    mass::{Mass, MassCalculator},
    sign::{sign, verify},
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
        UtxoEntry,
    },
};
use kaspa_txscript::pay_to_address_script;

use crate::{error::KwalletError, network::ALLOWED_NETWORK};

/// How many select-build-sign rounds `create_send` stries before giving up.
const MAX_FEE_ROUNDS: usize = 10;

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
        // sig_op_count is 1 because our pay-to-pubkey script does one Schnorr signature check.
        // sign() sets this field again anyway, and also fills in the empty signature_script.
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

pub fn sign_transaction(
    tx: SignableTransaction,
    key: &ExtendedPrivateKey<SecretKey>,
) -> Result<SignableTransaction, KwalletError> {
    let keypair = Keypair::from_secret_key(&Secp256k1::new(), key.private_key());
    let signed = sign(tx, keypair);
    verify(&signed.as_verifiable())?;

    Ok(signed)
}

/// The mass the network charges for a signed transaction: the largest of its compute,
/// transient and storage masses, each scaled to the compute-mass unit.
pub fn transaction_mass(tx: &SignableTransaction) -> Result<u64, KwalletError> {
    let params = Params::from(ALLOWED_NETWORK);
    let calculator = MassCalculator::new_with_consensus_params(&params);

    let non_contextual = calculator.calc_non_contextual_masses(&tx.tx);
    let contextual = calculator
        .calc_contextual_masses(&tx.as_verifiable())
        .ok_or(KwalletError::MassIncomputable)?;

    Ok(Mass::new(non_contextual, contextual).normalized_max(&params.block_mass_cofactors()))
}

/// Fee in sompi for `mass` grams at `feerate` sompi per gram, rounded up.
fn fee_for(mass: u64, feerate: f64) -> u64 {
    (mass as f64 * feerate).ceil() as u64
}

/// Select coins, build and sign a transaction that pays `amount` to `recipient`
/// with a fee of at lease mass x `feerate`.
///
/// `feerate` is in sompi per gram and must be finite and non-negative.
pub fn create_send(
    coins: &[Coin],
    amount: u64,
    recipient: &Address,
    change_address: &Address,
    key: &ExtendedPrivateKey<SecretKey>,
    feerate: f64,
) -> Result<SignableTransaction, KwalletError> {
    // Mass depends on the transaction and the fee changes the transaction: a higher fee can pull
    // in another coin (more inputs, more mass) or shrink the change (more storage mass). So we
    // repeat select-build-sign until the fee we paid covers the mass of what we built. A fixed
    // margin could still come up short, and an error would reject sends that one more round fixes.
    // The fee only ever goes up, so this ends with a match or InsufficientFunds. The round cap
    // is a safety net against a fee that creeps up by tiny steps.
    let mut fee = 0;
    for _ in 0..MAX_FEE_ROUNDS {
        let selection = select_coins(coins, amount, fee)?;
        let tx = sign_transaction(
            build_transaction(&selection, recipient, change_address),
            key,
        )?;

        let required = fee_for(transaction_mass(&tx)?, feerate);
        if fee >= required {
            return Ok(tx);
        }
        fee = required;
    }

    Err(KwalletError::FeeDidNotSettle)
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

    // A fresh random key. Every call gives a different one.
    fn new_key() -> ExtendedPrivateKey<SecretKey> {
        let master = master_key(&generate_mnemonic().unwrap()).unwrap();
        first_receive_key(master).unwrap()
    }

    // A coin locked to `key`'s address, so a signature by `key` can unblock it.
    fn owned_coin(amount: u64, id: u64, key: &ExtendedPrivateKey<SecretKey>) -> Coin {
        let script = pay_to_address_script(&receive_address(key).unwrap());
        Coin {
            outpoint: TransactionOutpoint::new(Hash::from_u64_word(id), 0),
            entry: UtxoEntry::new(amount, script, 0, false, None),
        }
    }

    fn fee_paid(tx: &SignableTransaction) -> u64 {
        let total_in: u64 = tx.entries.iter().map(|e| e.as_ref().unwrap().amount).sum();
        let total_out: u64 = tx.tx.outputs.iter().map(|o| o.value).sum();
        total_in - total_out
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

    // Guards against dropping the change output, which would hand it to the miner as fee.
    #[test]
    fn inputs_minus_outputs_equals_fee() {
        let fee = 1_000;
        let coins = [coin(3 * KAS, 1), coin(4 * KAS, 2)];
        let selection = select_coins(&coins, 5 * KAS, fee).unwrap();

        let tx = build_transaction(&selection, &address(1), &address(2));

        let total_in: u64 = tx.entries.iter().map(|e| e.as_ref().unwrap().amount).sum();
        let total_out: u64 = tx.tx.outputs.iter().map(|o| o.value).sum();
        assert_eq!(total_in - total_out, fee);
    }

    #[test]
    fn signed_transaction_passes_verify() {
        let key = new_key();
        let coins = [owned_coin(10 * KAS, 1, &key), owned_coin(5 * KAS, 2, &key)];
        let selection = select_coins(&coins, 12 * KAS, 1_000).unwrap();
        let unsigned = build_transaction(&selection, &address(1), &address(2));

        let signed = sign_transaction(unsigned, &key).unwrap();

        assert!(verify(&signed.as_verifiable()).is_ok());
        assert!(
            signed
                .tx
                .inputs
                .iter()
                .all(|i| !i.signature_script.is_empty())
        );
    }

    #[test]
    fn signing_with_another_key_fails_verify() {
        let owner = new_key();
        let thief = new_key();
        let coins = [owned_coin(10 * KAS, 1, &owner)];
        let selection = select_coins(&coins, 3 * KAS, 1_000).unwrap();
        let unsigned = build_transaction(&selection, &address(1), &address(2));

        assert!(matches!(
            sign_transaction(unsigned, &thief),
            Err(KwalletError::Signing(_))
        ));
    }

    #[test]
    fn fee_covers_mass_times_feerate() {
        let key = new_key();
        let coins = [owned_coin(10 * KAS, 1, &key)];
        let feerate = 1.5;

        let tx = create_send(&coins, 3 * KAS, &address(1), &address(2), &key, feerate).unwrap();

        let mass = transaction_mass(&tx).unwrap();
        assert!(fee_paid(&tx) >= fee_for(mass, feerate));
        assert_eq!(tx.tx.outputs[0].value, 3 * KAS);
    }

    // With no fee, one coin pays the amount exactly. The real fee then needs a second coin,
    // which adds mass, so the fee from the first round is too low and another round must run.
    #[test]
    fn fee_still_covers_mass_when_a_second_coin_is_needed() {
        let key = new_key();
        let coins = [owned_coin(KAS, 1, &key), owned_coin(KAS, 2, &key)];
        let feerate = 1.0;

        let tx = create_send(&coins, KAS, &address(1), &address(2), &key, feerate).unwrap();

        assert_eq!(tx.tx.inputs.len(), 2);
        let mass = transaction_mass(&tx).unwrap();
        assert!(fee_paid(&tx) >= fee_for(mass, feerate));
    }

    #[test]
    fn zero_feerate_means_zero_fee() {
        let key = new_key();
        let coins = [owned_coin(10 * KAS, 1, &key)];

        let tx = create_send(&coins, 3 * KAS, &address(1), &address(2), &key, 0.0).unwrap();

        assert_eq!(fee_paid(&tx), 0);
    }

    // Sending 1 KAS from a 1.00001 KAS coin leaves 1_000 sompi of change. Storage mass grows
    // with 1 / output value, so that tiny output makes the transaction enormously heavy.
    #[test]
    fn tiny_change_makes_storage_mass_explode() {
        let key = new_key();
        let coins = [owned_coin(KAS + 1_000, 1, &key)];

        let selection = select_coins(&coins, KAS, 0).unwrap();
        assert_eq!(selection.change, Some(1_000));
        let tx = sign_transaction(
            build_transaction(&selection, &address(1), &address(2)),
            &key,
        )
        .unwrap();
        let calculator = MassCalculator::new_with_consensus_params(&Params::from(ALLOWED_NETWORK));
        let storage_mass = calculator
            .calc_contextual_masses(&tx.as_verifiable())
            .unwrap()
            .storage_mass;
        let compute_mass = calculator.calc_non_contextual_masses(&tx.tx).compute_mass;
        println!("storage mass: {storage_mass}, compute mass: {compute_mass}");

        assert!(storage_mass > 100 * compute_mass);
        // At 1 sompi/gram the fee alone would be ~10 KAS, far more than the coin holds.
        assert!(matches!(
            create_send(&coins, KAS, &address(1), &address(2), &key, 1.0),
            Err(KwalletError::InsufficientFunds { .. })
        ));
    }
}
