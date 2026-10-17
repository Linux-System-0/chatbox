//! Access/refresh token issuance and verification.

use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// JWT claims carried by an access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the internal user id as a string.
    pub sub: String,
    /// Session id (`jti`).
    pub sid: String,
    /// Device id.
    pub did: i64,
    /// Issued-at (Unix seconds).
    pub iat: usize,
    /// Expiry (Unix seconds).
    pub exp: usize,
    /// Token kind; always `access`.
    pub kind: String,
}

impl Claims {
    /// The subject parsed back into a user id.
    pub fn user_id(&self) -> Result<i64> {
        self.sub.parse().map_err(|_| Error::Unauthorized)
    }
}

/// Configuration for token lifetimes and signing.
#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// Secret used for HMAC signing. Must be non-empty in production.
    pub secret: String,
    /// Access token lifetime.
    pub access_ttl: Duration,
    /// Refresh token lifetime.
    pub refresh_ttl: Duration,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            secret: "chatbox-development-secret".into(),
            access_ttl: Duration::from_secs(15 * 60),
            refresh_ttl: Duration::from_secs(30 * 24 * 60 * 60),
        }
    }
}

/// Issues and verifies JWTs, and generates opaque refresh tokens.
pub struct TokenManager {
    encoding: EncodingKey,
    decoding: DecodingKey,
    config: TokenConfig,
}

impl TokenManager {
    /// Create a manager from the given configuration.
    pub fn new(config: TokenConfig) -> Self {
        let bytes = config.secret.as_bytes();
        Self {
            encoding: EncodingKey::from_secret(bytes),
            decoding: DecodingKey::from_secret(bytes),
            config,
        }
    }

    /// The configured refresh token lifetime.
    pub fn refresh_ttl(&self) -> Duration {
        self.config.refresh_ttl
    }

    /// The configured access token lifetime.
    pub fn access_ttl(&self) -> Duration {
        self.config.access_ttl
    }

    /// Issue a signed access token for a session.
    pub fn issue_access(&self, user_id: i64, device_id: i64, session_id: &str) -> Result<String> {
        let now = Utc::now();
        let exp = now
            + chrono::Duration::from_std(self.config.access_ttl)
                .map_err(|e| Error::Internal(e.to_string()))?;
        let claims = Claims {
            sub: user_id.to_string(),
            sid: session_id.to_string(),
            did: device_id,
            iat: now.timestamp() as usize,
            exp: exp.timestamp() as usize,
            kind: "access".into(),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(|e| Error::Internal(format!("failed to sign token: {e}")))
    }

    /// Verify an access token, returning its claims.
    pub fn verify_access(&self, token: &str) -> Result<Claims> {
        let validation = Validation::new(Algorithm::HS256);
        let data = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|_| Error::Unauthorized)?;
        if data.claims.kind != "access" {
            return Err(Error::Unauthorized);
        }
        Ok(data.claims)
    }

    /// Generate a random opaque refresh token and return `(raw, hash)`.
    pub fn issue_refresh(&self) -> (String, String) {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let raw = base64_url(&bytes);
        let hash = hash_refresh(&raw);
        (raw, hash)
    }
}

/// Hash a refresh token for storage.
pub fn hash_refresh(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_token_roundtrip() {
        let mgr = TokenManager::new(TokenConfig::default());
        let token = mgr.issue_access(42, 7, "sess-1").unwrap();
        let claims = mgr.verify_access(&token).unwrap();
        assert_eq!(claims.user_id().unwrap(), 42);
        assert_eq!(claims.did, 7);
        assert_eq!(claims.sid, "sess-1");
    }

    #[test]
    fn tampered_token_is_rejected() {
        let mgr = TokenManager::new(TokenConfig::default());
        let token = mgr.issue_access(42, 7, "sess-1").unwrap();
        let other = TokenManager::new(TokenConfig {
            secret: "different".into(),
            ..Default::default()
        });
        assert!(other.verify_access(&token).is_err());
    }

    #[test]
    fn refresh_tokens_are_unique_and_hashed() {
        let mgr = TokenManager::new(TokenConfig::default());
        let (a, ha) = mgr.issue_refresh();
        let (b, hb) = mgr.issue_refresh();
        assert_ne!(a, b);
        assert_ne!(ha, hb);
        assert_eq!(ha, hash_refresh(&a));
    }
}
