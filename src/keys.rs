use kaspa_bip32::{Language::English, Mnemonic, WordCount::Words24};

use crate::error::KwalletError;

/// Generate a fresh random 24-word BIP39 mnemonic phrase.
pub fn generate_phrase() -> Result<String, KwalletError> {
    let mnemonic = Mnemonic::random(Words24, English)?;

    Ok(mnemonic.phrase().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_24_words() {
        let phrase = generate_phrase().unwrap();

        assert_eq!(phrase.split_whitespace().count(), 24);
    }

    #[test]
    fn generate_different_phrases() {
        assert_ne!(generate_phrase().unwrap(), generate_phrase().unwrap());
    }
}
