use crate::core::{PebbleError, Result};
use rand::RngCore;
use tracing::{info, warn};
use zeroize::Zeroizing;

const SERVICE_NAME: &str = "com.pebble.email";
const KEY_ENTRY: &str = "master-dek";
const DEK_LEN: usize = 32;

pub struct KeyStore;

trait DekCredential {
    fn get_secret(&self) -> std::result::Result<Vec<u8>, keyring::Error>;
    fn set_secret(&self, secret: &[u8]) -> std::result::Result<(), keyring::Error>;
}

impl DekCredential for keyring::Entry {
    fn get_secret(&self) -> std::result::Result<Vec<u8>, keyring::Error> {
        keyring::Entry::get_secret(self)
    }

    fn set_secret(&self, secret: &[u8]) -> std::result::Result<(), keyring::Error> {
        keyring::Entry::set_secret(self, secret)
    }
}

impl KeyStore {
    /// Get or create the Data Encryption Key from the OS credential store.
    ///
    /// The raw 32-byte key is hex-encoded before storing so it can round-trip
    /// safely through string-based keychain backends and survive kernel-keyring
    /// serialisation.
    pub fn get_or_create_dek() -> Result<Zeroizing<[u8; DEK_LEN]>> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_ENTRY)
            .map_err(|e| PebbleError::Auth(format!("Keyring entry error: {e}")))?;

        get_or_create_dek_from_credential(&entry)
    }

    /// Delete the DEK from the OS credential store.
    pub fn delete_dek() -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_ENTRY)
            .map_err(|e| PebbleError::Auth(format!("Keyring entry error: {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already gone
            Err(e) => Err(PebbleError::Auth(format!("Failed to delete DEK: {e}"))),
        }
    }
}

/// Decode a 32-byte key from its hex representation.
fn decode_hex(hex_data: &[u8]) -> std::result::Result<Zeroizing<[u8; DEK_LEN]>, ()> {
    let hex_str = std::str::from_utf8(hex_data).map_err(|_| ())?;
    let bytes = Zeroizing::new(hex::decode(hex_str).map_err(|_| ())?);
    if bytes.len() != DEK_LEN {
        return Err(());
    }
    let mut key = Zeroizing::new([0u8; DEK_LEN]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn get_or_create_dek_from_credential(
    credential: &impl DekCredential,
) -> Result<Zeroizing<[u8; DEK_LEN]>> {
    match credential.get_secret() {
        Ok(secret) => {
            let secret = Zeroizing::new(secret);
            if secret.len() == DEK_LEN {
                let mut key = Zeroizing::new([0u8; DEK_LEN]);
                key.copy_from_slice(&secret);
                let hex_key = Zeroizing::new(hex::encode(&key[..]));
                if let Err(error) = credential.set_secret(hex_key.as_bytes()) {
                    warn!("Failed to migrate legacy DEK to hex encoding: {error}");
                }
                return Ok(key);
            }

            if let Ok(key) = decode_hex(&secret) {
                return Ok(key);
            }

            Err(PebbleError::Auth(format!(
                "Stored DEK has an invalid format (len={}); refusing to overwrite it",
                secret.len()
            )))
        }
        Err(keyring::Error::NoEntry) => {
            info!("No DEK found, generating new one");
            generate_and_store_dek(credential)
        }
        Err(e) => Err(PebbleError::Auth(format!("Keyring read error: {e}"))),
    }
}

fn generate_and_store_dek(credential: &impl DekCredential) -> Result<Zeroizing<[u8; DEK_LEN]>> {
    let mut key = Zeroizing::new([0u8; DEK_LEN]);
    rand::thread_rng().fill_bytes(&mut *key);
    let hex_key = Zeroizing::new(hex::encode(&key[..]));
    credential
        .set_secret(hex_key.as_bytes())
        .map_err(|e| PebbleError::Auth(format!("Failed to store DEK: {e}")))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct FakeCredential {
        secret: RefCell<Option<Vec<u8>>>,
        writes: Cell<usize>,
    }

    impl FakeCredential {
        fn with_secret(secret: Vec<u8>) -> Self {
            Self {
                secret: RefCell::new(Some(secret)),
                writes: Cell::new(0),
            }
        }

        fn without_secret() -> Self {
            Self {
                secret: RefCell::new(None),
                writes: Cell::new(0),
            }
        }

        fn stored_key(&self) -> Zeroizing<[u8; DEK_LEN]> {
            decode_hex(self.secret.borrow().as_deref().unwrap()).unwrap()
        }
    }

    impl DekCredential for FakeCredential {
        fn get_secret(&self) -> std::result::Result<Vec<u8>, keyring::Error> {
            self.secret.borrow().clone().ok_or(keyring::Error::NoEntry)
        }

        fn set_secret(&self, secret: &[u8]) -> std::result::Result<(), keyring::Error> {
            self.writes.set(self.writes.get() + 1);
            self.secret.borrow_mut().replace(secret.to_vec());
            Ok(())
        }
    }

    #[test]
    fn invalid_stored_dek_is_rejected_without_overwriting_it() {
        let credential = FakeCredential::with_secret(vec![7u8; 50]);

        let error = get_or_create_dek_from_credential(&credential).unwrap_err();

        assert!(error.to_string().contains("invalid format"));
        assert_eq!(credential.secret.borrow().as_deref(), Some(&[7u8; 50][..]));
        assert_eq!(credential.writes.get(), 0);
    }

    #[test]
    fn legacy_raw_dek_is_returned_and_migrated_to_hex() {
        let raw_key: Vec<u8> = (0..DEK_LEN as u8).collect();
        let credential = FakeCredential::with_secret(raw_key.clone());

        let dek = get_or_create_dek_from_credential(&credential).unwrap();

        assert_eq!(&dek[..], raw_key.as_slice());
        assert_eq!(
            credential.secret.borrow().as_deref().unwrap(),
            hex::encode(raw_key).as_bytes()
        );
        assert_eq!(credential.writes.get(), 1);
    }

    #[test]
    fn valid_hex_dek_is_returned_without_rewriting() {
        let raw_key = [0xa5; DEK_LEN];
        let credential = FakeCredential::with_secret(hex::encode(raw_key).into_bytes());

        let dek = get_or_create_dek_from_credential(&credential).unwrap();

        assert_eq!(&*dek, &raw_key);
        assert_eq!(credential.writes.get(), 0);
    }

    #[test]
    fn missing_dek_is_generated_and_stored_as_hex() {
        let credential = FakeCredential::without_secret();

        let dek = get_or_create_dek_from_credential(&credential).unwrap();

        assert_eq!(
            credential.secret.borrow().as_ref().unwrap().len(),
            DEK_LEN * 2
        );
        assert_eq!(&*credential.stored_key(), &*dek);
        assert_eq!(credential.writes.get(), 1);
    }
}
