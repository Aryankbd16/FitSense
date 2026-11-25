use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::wardrobe::{Error, Result};

pub type Secret = Zeroizing<[u8; 32]>;
const RECOVERY_MAGIC: &[u8; 8] = b"FSRECV01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const WRAP_LEN: usize = RECOVERY_MAGIC.len() + SALT_LEN + NONCE_LEN + 32 + 16;
pub const RECOVERY_LEN: usize = WRAP_LEN + 32;

pub fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| Error::EncryptionUnavailable)?;
    Ok(bytes)
}

pub fn derive(master: &Secret, purpose: &[u8]) -> Result<Secret> {
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(b"FitSense vault v1"), master.as_ref())
        .expand(purpose, key.as_mut())
        .map_err(|_| Error::EncryptionUnavailable)?;
    Ok(key)
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    result
}

pub fn seal(key: &Secret, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let nonce: [u8; NONCE_LEN] = random()?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| Error::EncryptionUnavailable)?;
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::EncryptionUnavailable)?;
    let mut result = Vec::with_capacity(nonce.len() + encrypted.len());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&encrypted);
    Ok(result)
}

pub fn unseal(key: &Secret, aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    let (nonce, ciphertext) = bytes.split_at_checked(NONCE_LEN).ok_or(Error::Integrity)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| Error::EncryptionUnavailable)?;
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| Error::Integrity)
}

fn derive_wrapping_key(passphrase: &str, salt: &[u8]) -> Result<Secret> {
    let params =
        Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| Error::EncryptionUnavailable)?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::EncryptionUnavailable)?;
    Ok(key)
}

fn validate_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < 12 || passphrase.len() > 256 || passphrase.trim().is_empty() {
        return Err(Error::InvalidPassphrase);
    }
    Ok(())
}

pub fn recovery(master: &Secret, passphrase: &str) -> Result<Vec<u8>> {
    validate_passphrase(passphrase)?;
    let salt: [u8; SALT_LEN] = random()?;
    let mut header = RECOVERY_MAGIC.to_vec();
    header.extend_from_slice(&salt);
    let wrapped = seal(
        &derive_wrapping_key(passphrase, &salt)?,
        &header,
        master.as_ref(),
    )?;
    header.extend_from_slice(&wrapped);
    let mut mac = recovery_mac(master)?;
    mac.update(&header);
    header.extend_from_slice(&mac.finalize().into_bytes());
    Ok(header)
}

fn recovery_mac(master: &Secret) -> Result<Hmac<Sha256>> {
    <Hmac<Sha256> as Mac>::new_from_slice(derive(master, b"recovery envelope")?.as_ref())
        .map_err(|_| Error::EncryptionUnavailable)
}

pub fn verify_recovery(envelope: &[u8], master: &Secret) -> Result<()> {
    if envelope.len() != RECOVERY_LEN || !envelope.starts_with(RECOVERY_MAGIC) {
        return Err(Error::Incompatible);
    }
    let (bytes, tag) = envelope.split_at(WRAP_LEN);
    let mut mac = recovery_mac(master)?;
    mac.update(bytes);
    mac.verify_slice(tag).map_err(|_| Error::Integrity)
}

pub fn recover(envelope: &[u8], passphrase: &str) -> Result<Secret> {
    validate_passphrase(passphrase)?;
    if envelope.len() != RECOVERY_LEN || !envelope.starts_with(RECOVERY_MAGIC) {
        return Err(Error::Incompatible);
    }
    let (header, wrapped) = envelope[..WRAP_LEN].split_at(RECOVERY_MAGIC.len() + SALT_LEN);
    let key = derive_wrapping_key(passphrase, &header[RECOVERY_MAGIC.len()..])?;
    let plaintext =
        Zeroizing::new(unseal(&key, header, wrapped).map_err(|_| Error::WrongPassphrase)?);
    let mut secret = Zeroizing::new([0; 32]);
    secret.copy_from_slice(&plaintext);
    verify_recovery(envelope, &secret)?;
    Ok(secret)
}

#[cfg(windows)]
fn dpapi(input: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>> {
    use std::{ptr, slice};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    use zeroize::Zeroize;

    let input = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // DPAPI allocates the output with LocalAlloc and owns neither input nor entropy.
    let success = unsafe {
        if protect {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 || output.pbData.is_null() {
        return Err(Error::LocalKey);
    }
    let copy = unsafe {
        let buffer = slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copy = Zeroizing::new(buffer.to_vec());
        buffer.zeroize();
        LocalFree(output.pbData.cast());
        copy
    };
    Ok(copy)
}

#[cfg(not(windows))]
fn dpapi(_: &[u8], _: bool) -> Result<Zeroizing<Vec<u8>>> {
    Err(Error::EncryptionUnavailable)
}

pub fn protect_local(master: &Secret) -> Result<Zeroizing<Vec<u8>>> {
    dpapi(master.as_ref(), true)
}

pub fn unprotect_local(wrapped: &[u8]) -> Result<Secret> {
    let bytes = dpapi(wrapped, false)?;
    if bytes.len() != 32 {
        return Err(Error::LocalKey);
    }
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_keys_are_separate_and_ciphertext_is_bound_to_its_slot() {
        let master = Zeroizing::new(random::<32>().unwrap());
        let media = derive(&master, b"media").unwrap();
        let archive = derive(&master, b"archive").unwrap();
        let a = seal(&media, b"slot1", b"private").unwrap();
        let b = seal(&media, b"slot1", b"private").unwrap();
        assert_ne!(a, b);
        assert_ne!(*media, *archive);
        assert_eq!(unseal(&media, b"slot1", &a).unwrap(), b"private");
        assert_eq!(unseal(&media, b"slot2", &a), Err(Error::Integrity));
    }

    #[test]
    fn recovery_authenticates_passphrase_and_header() {
        let master = Zeroizing::new(random::<32>().unwrap());
        let pass = "a test phrase with spaces";
        let mut envelope = recovery(&master, pass).unwrap();
        assert_eq!(*recover(&envelope, pass).unwrap(), *master);
        assert!(matches!(
            recover(&envelope, "incorrect test phrase"),
            Err(Error::WrongPassphrase)
        ));
        envelope[9] ^= 1;
        assert!(matches!(
            recover(&envelope, pass),
            Err(Error::WrongPassphrase)
        ));
        assert!(matches!(
            recovery(&master, "weak"),
            Err(Error::InvalidPassphrase)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_current_user_authenticates_the_local_wrapper() {
        let master = Zeroizing::new(random::<32>().unwrap());
        let mut protected = protect_local(&master).unwrap();
        assert_eq!(*unprotect_local(&protected).unwrap(), *master);
        protected[20] ^= 1;
        assert!(matches!(unprotect_local(&protected), Err(Error::LocalKey)));
    }
}
