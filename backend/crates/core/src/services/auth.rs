//! Authentication: registration, login, token refresh, and session lifecycle.

use std::sync::OnceLock;

use regex::Regex;

use super::Services;
use crate::error::{Error, Result};
use crate::model::{Device, Platform, Session, User};
use crate::password::{hash_password, verify_password};
use crate::tokens::hash_refresh;

/// Input for account registration.
#[derive(Debug, Clone)]
pub struct RegisterInput {
    /// Custom public user id.
    pub user_id: String,
    /// Plaintext password.
    pub password: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// Device label.
    pub device_name: String,
    /// Device platform.
    pub platform: Platform,
}

/// Input for login.
#[derive(Debug, Clone)]
pub struct LoginInput {
    /// Custom public user id.
    pub user_id: String,
    /// Plaintext password.
    pub password: String,
    /// Device label.
    pub device_name: String,
    /// Device platform.
    pub platform: Platform,
}

/// Issued credentials.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthTokens {
    /// Signed access token.
    pub access_token: String,
    /// Opaque refresh token.
    pub refresh_token: String,
    /// Session id.
    pub session_id: String,
    /// Bound device id.
    pub device_id: i64,
    /// Access token lifetime in seconds.
    pub expires_in: u64,
}

/// The identity derived from a verified access token.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Internal user id.
    pub user_id: i64,
    /// Bound device id.
    pub device_id: i64,
    /// Session id.
    pub session_id: String,
}

fn user_id_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9_.-]+$").expect("valid user id regex"))
}

impl Services {
    /// Validate a custom user id against the length and character rules.
    pub fn validate_user_id(&self, user_id: &str) -> Result<()> {
        let len = user_id.chars().count();
        if len < self.config.min_user_id_len || len > self.config.max_user_id_len {
            return Err(Error::Invalid(format!(
                "user id must be between {} and {} characters",
                self.config.min_user_id_len, self.config.max_user_id_len
            )));
        }
        if !user_id_regex().is_match(user_id) {
            return Err(Error::Invalid(
                "user id may only contain letters, digits, '.', '_' and '-'".into(),
            ));
        }
        Ok(())
    }

    /// Register a new account, device, and session.
    pub async fn register(&self, input: RegisterInput) -> Result<(User, AuthTokens)> {
        self.validate_user_id(&input.user_id)?;
        if input.password.chars().count() < 8 {
            return Err(Error::Invalid(
                "password must be at least 8 characters".into(),
            ));
        }
        if self.stores.users.exists_user_id(&input.user_id).await? {
            return Err(Error::Conflict("user id is already taken".into()));
        }
        let now = self.now();
        let user = User {
            id: self.new_id(),
            user_id: input.user_id.clone(),
            display_name: input.display_name.filter(|s| !s.trim().is_empty()),
            avatar_url: None,
            password_hash: hash_password(&input.password)?,
            created_at: now,
            updated_at: now,
        };
        self.stores.users.create(&user).await?;
        let tokens = self
            .start_session(&user, &input.device_name, input.platform)
            .await?;
        Ok((user, tokens))
    }

    /// Verify credentials and start a new session.
    pub async fn login(&self, input: LoginInput) -> Result<(User, AuthTokens)> {
        let user = self
            .stores
            .users
            .get_by_user_id(&input.user_id)
            .await?
            .ok_or(Error::Unauthorized)?;
        if !verify_password(&user.password_hash, &input.password)? {
            return Err(Error::Unauthorized);
        }
        let tokens = self
            .start_session(&user, &input.device_name, input.platform)
            .await?;
        Ok((user, tokens))
    }

    async fn start_session(
        &self,
        user: &User,
        device_name: &str,
        platform: Platform,
    ) -> Result<AuthTokens> {
        let now = self.now();
        let device = Device {
            id: self.new_id(),
            user_id: user.id,
            name: if device_name.trim().is_empty() {
                "unknown".into()
            } else {
                device_name.trim().to_string()
            },
            platform,
            last_seen_at: now,
            created_at: now,
        };
        self.stores.devices.create(&device).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let (secret, secret_hash) = self.tokens.issue_refresh();
        let ttl = self.tokens.refresh_ttl();
        let expires_at =
            now + chrono::Duration::from_std(ttl).map_err(|e| Error::Internal(e.to_string()))?;
        let session = Session {
            id: session_id.clone(),
            user_id: user.id,
            device_id: device.id,
            refresh_hash: secret_hash,
            issued_at: now,
            expires_at,
            revoked_at: None,
        };
        self.stores.sessions.create(&session).await?;
        self.issue_tokens(&session, &secret)
    }

    fn issue_tokens(&self, session: &Session, refresh_secret: &str) -> Result<AuthTokens> {
        let access_token =
            self.tokens
                .issue_access(session.user_id, session.device_id, &session.id)?;
        Ok(AuthTokens {
            access_token,
            refresh_token: format!("{}:{}", session.id, refresh_secret),
            session_id: session.id.clone(),
            device_id: session.device_id,
            expires_in: self.tokens.access_ttl().as_secs(),
        })
    }

    /// Exchange a refresh token for a new token pair, rotating the refresh
    /// secret.
    pub async fn refresh(&self, refresh_token: &str) -> Result<AuthTokens> {
        let (session_id, secret) = refresh_token.split_once(':').ok_or(Error::Unauthorized)?;
        let mut session = self
            .stores
            .sessions
            .get(session_id)
            .await?
            .ok_or(Error::Unauthorized)?;
        let now = self.now();
        if !session.is_active(now) {
            return Err(Error::Unauthorized);
        }
        if hash_refresh(secret) != session.refresh_hash {
            return Err(Error::Unauthorized);
        }
        let (new_secret, new_hash) = self.tokens.issue_refresh();
        session.refresh_hash = new_hash;
        self.stores.sessions.update(&session).await?;
        self.issue_tokens(&session, &new_secret)
    }

    /// Revoke a session.
    pub async fn logout(&self, session_id: &str) -> Result<()> {
        if let Some(mut session) = self.stores.sessions.get(session_id).await? {
            session.revoked_at = Some(self.now());
            self.stores.sessions.update(&session).await?;
        }
        Ok(())
    }

    /// Verify an access token and confirm its session is still active.
    pub async fn authenticate(&self, token: &str) -> Result<AuthContext> {
        let claims = self.tokens.verify_access(token)?;
        let session = self
            .stores
            .sessions
            .get(&claims.sid)
            .await?
            .ok_or(Error::Unauthorized)?;
        if !session.is_active(self.now()) {
            return Err(Error::Unauthorized);
        }
        let user_id = claims.user_id()?;
        if user_id != session.user_id {
            return Err(Error::Unauthorized);
        }
        Ok(AuthContext {
            user_id,
            device_id: session.device_id,
            session_id: session.id,
        })
    }

    /// List a user's active sessions.
    pub async fn list_sessions(&self, user_id: i64) -> Result<Vec<Session>> {
        self.stores.sessions.list_for_user(user_id).await
    }

    /// Fetch the authenticated user record.
    pub async fn current_user(&self, ctx: &AuthContext) -> Result<User> {
        self.stores
            .users
            .get(ctx.user_id)
            .await?
            .ok_or(Error::NotFound)
    }
}
