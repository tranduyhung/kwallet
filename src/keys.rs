use kaspa_bip32::{
    DerivationPath, ExtendedPrivateKey, Language::English, Mnemonic, SecretKey, WordCount::Words24,
};

use crate::error::KwalletError;

/// Generate a fresh random 24-word BIP39 mnemonic.
pub fn generate_mnemonic() -> Result<Mnemonic, KwalletError> {
    let mnemonic = Mnemonic::random(Words24, English)?;

    Ok(mnemonic)
}

/// Derive the root (master) extended private key from a mnemonic, without a passphrase.
pub fn master_key(mnemonic: &Mnemonic) -> Result<ExtendedPrivateKey<SecretKey>, KwalletError> {
    let seed = mnemonic.to_seed("");
    let master = ExtendedPrivateKey::new(seed)?;

    Ok(master)
}

/// Derive the key for Kaspa's first receive address (m/44'/111111'/0'/0/0).
pub fn first_receive_key(
    master: ExtendedPrivateKey<SecretKey>,
) -> Result<ExtendedPrivateKey<SecretKey>, KwalletError> {
    let path: DerivationPath = "m/44'/111111'/0'/0/0".parse()?;
    let child = master.derive_path(&path)?;

    Ok(child)
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

    #[test]
    fn first_receive_key_differs_from_master() {
        let mnemonic = generate_mnemonic().unwrap();
        let master = master_key(&mnemonic).unwrap();

        let child = first_receive_key(master.clone());

        assert!(child.is_ok());
        assert_ne!(child.unwrap(), master);
    }
}
