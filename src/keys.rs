use kaspa_addresses::{Address, Version};
use kaspa_bip32::{
    DerivationPath, ExtendedPrivateKey, Language::English, Mnemonic, SecretKey, WordCount::Words24,
};

use crate::{error::KwalletError, network::ALLOWED_NETWORK};

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

/// Build the Schnorr (x-only) devnet address for a derived key.
pub fn receive_address(
    extended_private_key: &ExtendedPrivateKey<SecretKey>,
) -> Result<Address, KwalletError> {
    let extended_public_key = extended_private_key.public_key();
    let public_key = extended_public_key.public_key();
    let (x_only, _parity) = public_key.x_only_public_key();

    let address = Address::try_new(ALLOWED_NETWORK.into(), Version::PubKey, &x_only.serialize())?;

    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derived_key() -> ExtendedPrivateKey<SecretKey> {
        let mnemonic = generate_mnemonic().unwrap();
        let master = master_key(&mnemonic).unwrap();

        first_receive_key(master).unwrap()
    }

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

    #[test]
    fn receive_address_has_devnet_prefix() {
        let address = receive_address(&derived_key()).unwrap();

        assert!(address.to_string().starts_with("kaspadev:"));
    }

    // The text `kwallet new` prints must decode back to the very same address.
    #[test]
    fn receive_address_round_trip() {
        let address = receive_address(&derived_key()).unwrap();

        // Address -> text, exactly what the user sees and copies.
        let text = address.to_string();

        // Text -> Address, as a wallet receiving that text would parse it.
        let parsed = Address::try_from(text.as_str()).unwrap();

        // Same prefix, same version, same 32-byte payload.
        assert_eq!(parsed, address);
    }

    // The address must hold the key's x coordinate, not just any 32 bytes.
    #[test]
    fn receive_address_payload_is_x_coordinate() {
        let key = derived_key();

        // The compressed public key is 33 bytes: [0x02 or 0x03, x (32 bytes)].
        let compressed = key.public_key().public_key().serialize();
        assert!(compressed[0] == 0x02 || compressed[0] == 0x03);
        let x = &compressed[1..];

        let address = receive_address(&key).unwrap();

        // Skipping the prefix byte leaves x, which must equal the payload.
        // Taking the first 32 bytes instead would include 0x02/0x03 and fail here.
        assert_eq!(address.payload.as_slice(), x);
    }
}
