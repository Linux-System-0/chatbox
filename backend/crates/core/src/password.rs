//! Password hashing and verification (Argon2id).

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::{Error, Result};

/// Hash a plaintext password with Argon2id and a random salt.
pub fn hash_password(password: &str) -> Result<String> {
    if password.is_empty() {
        return Err(Error::Invalid("password must not be empty".into()));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("password hashing failed: {e}")))
}

/// Verify a plaintext password against a stored Argon2id hash.
pub fn verify_password(hash: &str, password: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| Error::Internal(format!("invalid password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password(&hash, "correct horse battery staple").unwrap());
        assert!(!verify_password(&hash, "wrong").unwrap());
        assert_ne!(hash, hash_password("correct horse battery staple").unwrap());
    }

    #[test]
    fn empty_password_is_rejected() {
        assert!(hash_password("").is_err());
    }
}
