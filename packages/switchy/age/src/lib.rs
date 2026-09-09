//! Feature-selected SSH recipient wrapping. Simulator envelopes are not encryption.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(not(any(feature = "native", feature = "simulator")))]
compile_error!("select native or simulator");

/// Secret-safe wrapping failure.
#[derive(Debug, thiserror::Error)]
#[error("recipient wrapping or identity verification failed")]
pub struct Error;

/// Wrap bytes for an SSH public key (a synthetic identity in simulator builds).
///
/// # Errors
/// Returns an error for invalid recipients or encryption failures.
pub fn wrap(recipient: &str, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    backend::wrap(recipient, plaintext)
}

/// Unwrap bytes using an SSH private key (the same synthetic identity in simulation).
///
/// # Errors
/// Returns an error for invalid keys, wrong identities, or damaged envelopes.
pub fn unwrap(identity: &str, ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
    backend::unwrap(identity, ciphertext)
}

#[cfg(all(feature = "native", not(feature = "simulator")))]
mod backend {
    use super::Error;
    use std::io::{Read as _, Write as _};

    pub fn wrap(recipient: &str, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let recipient: age::ssh::Recipient = recipient.parse().map_err(|_| Error)?;
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(|_| Error)?;
        let mut result = Vec::new();
        let mut writer = encryptor.wrap_output(&mut result).map_err(|_| Error)?;
        writer.write_all(plaintext).map_err(|_| Error)?;
        writer.finish().map_err(|_| Error)?;
        Ok(result)
    }

    pub fn unwrap(identity: &str, ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let identity =
            age::ssh::Identity::from_buffer(std::io::Cursor::new(identity.as_bytes()), None)
                .map_err(|_| Error)?;
        let decryptor = age::Decryptor::new(ciphertext).map_err(|_| Error)?;
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|_| Error)?;
        let mut result = Vec::new();
        reader.read_to_end(&mut result).map_err(|_| Error)?;
        Ok(result)
    }
}

#[cfg(feature = "simulator")]
mod backend {
    use super::Error;
    use sha2::{Digest as _, Sha256};
    const MAGIC: &[u8] = b"switchy-age-simulation-v1\0";

    fn identity_digest(identity: &str) -> Result<[u8; 32], Error> {
        let name = identity
            .strip_prefix("sim-age:")
            .filter(|name| !name.is_empty())
            .ok_or(Error)?;
        Ok(Sha256::digest(name.as_bytes()).into())
    }

    pub fn wrap(recipient: &str, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut result = MAGIC.to_vec();
        result.extend_from_slice(&identity_digest(recipient)?);
        result.extend_from_slice(plaintext);
        let checksum = Sha256::digest(&result);
        result.extend_from_slice(&checksum);
        Ok(result)
    }

    pub fn unwrap(identity: &str, ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        if !ciphertext.starts_with(MAGIC) || ciphertext.len() < MAGIC.len() + 64 {
            return Err(Error);
        }
        let end = ciphertext.len() - 32;
        if ciphertext[MAGIC.len()..MAGIC.len() + 32] != identity_digest(identity)?
            || ciphertext[end..] != Sha256::digest(&ciphertext[..end])[..]
        {
            return Err(Error);
        }
        Ok(ciphertext[MAGIC.len() + 32..end].to_vec())
    }
}

#[cfg(all(test, feature = "simulator"))]
mod tests {
    #[test]
    fn deterministic_identity_bound_envelopes_reject_damage_and_native_inputs() {
        let first = super::wrap("sim-age:alice", b"synthetic").unwrap();
        assert_eq!(first, super::wrap("sim-age:alice", b"synthetic").unwrap());
        assert_eq!(
            super::unwrap("sim-age:alice", &first).unwrap(),
            b"synthetic"
        );
        assert!(super::unwrap("sim-age:bob", &first).is_err());
        let mut damaged = first;
        *damaged.last_mut().unwrap() ^= 1;
        assert!(super::unwrap("sim-age:alice", &damaged).is_err());
        assert!(super::wrap("ssh-ed25519 real", b"secret").is_err());
        assert!(super::unwrap("sim-age:alice", b"age-encryption.org/v1").is_err());
    }
}
