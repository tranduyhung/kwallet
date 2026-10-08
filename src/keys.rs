use kaspa_bip32::{Language::English, Mnemonic, WordCount::Words24};

use crate::error::KwalletError;

/// Generate a fresh random 24-word BIP39 mnemonic.
pub fn generate_mnemonic() -> Result<Mnemonic, KwalletError> {
    let mnemonic = Mnemonic::random(Words24, English)?;

    Ok(mnemonic)
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
}
