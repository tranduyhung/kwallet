use kaspa_bip32::{ExtendedPrivateKey, Language::English, Mnemonic, SecretKey, WordCount::Words24};

use crate::error::KwalletError;

/// Generate a fresh random 24-word BIP39 mnemonic.
pub fn generate_mnemonic() -> Result<Mnemonic, KwalletError> {
    let mnemonic = Mnemonic::random(Words24, English)?;

    Ok(mnemonic)
}

/// Derive the root (master) extended private key from a mnemonic, without a passphrase.
pub fn master_key(mnemonic: &Mnemonic) -> Result<ExtendedPrivateKey<SecretKey>, KwalletError> {
    let seed = mnemonic.to_seed("");
    let master = ExtendedPrivateKey::<SecretKey>::new(seed)?;

    Ok(master)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_24_words() {
        let mnemonic = generate_mnemonic().unwrap();

        assert_eq!(mnemonic.phrase().split_whitespace().count(), 24);
    }

    #[test]
    fn generate_different_phrases() {
        let a = generate_mnemonic().unwrap();
        let b = generate_mnemonic().unwrap();

        assert_ne!(a.phrase(), b.phrase());
    }

    #[test]
    fn generated_phrase_is_valid() {
        let mnemonic = generate_mnemonic().unwrap();

        assert!(Mnemonic::validate(mnemonic.phrase(), Some(English)));
    }

    #[test]
    fn master_key_from_generated_mnemonic() {
        let mnemonic = generate_mnemonic().unwrap();

        assert!(master_key(&mnemonic).is_ok());
    }
}
