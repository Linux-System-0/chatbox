//! Business services. Each submodule extends [`Services`] with a cohesive set
//! of operations grouped by domain area.

mod attachment;
mod auth;
mod block;
mod contact;
mod group;
mod messaging;
mod search;
mod user;

use std::sync::Arc;

use crate::clock::Clock;
use crate::error::Result;
use crate::ids::IdGenerator;
use crate::infra::Infra;
use crate::model::Timestamp;
use crate::port::Stores;
use crate::tokens::TokenManager;

pub use auth::{AuthContext, AuthTokens, LoginInput, RegisterInput};
pub use block::BlockView;
pub use group::{GroupMemberView, GroupView};
pub use messaging::{ConversationView, SendRequest};
pub use search::wildcard_to_regex;

/// Tunable service limits.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Minimum custom user id length.
    pub min_user_id_len: usize,
    /// Maximum custom user id length.
    pub max_user_id_len: usize,
    /// Maximum message body length.
    pub max_message_len: usize,
    /// Maximum group name length.
    pub max_group_name_len: usize,
    /// Default page size for message history.
    pub default_page_size: u32,
    /// Maximum page size for message history.
    pub max_page_size: u32,
    /// Groups at or below this size fan out on write; larger ones on read.
    pub fanout_write_max_members: u64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            min_user_id_len: 3,
            max_user_id_len: 32,
            max_message_len: 8000,
            max_group_name_len: 64,
            default_page_size: 50,
            max_page_size: 200,
            fanout_write_max_members: 256,
        }
    }
}

/// The application service facade. It bundles the repository ports, the
/// infrastructure ports, and the pure helpers (ids, clock, token signing).
#[derive(Clone)]
pub struct Services {
    /// Repository ports.
    pub stores: Stores,
    /// Infrastructure ports.
    pub infra: Infra,
    /// Identifier generator.
    pub ids: Arc<dyn IdGenerator>,
    /// Token manager.
    pub tokens: Arc<TokenManager>,
    /// Clock.
    pub clock: Arc<dyn Clock>,
    /// Service limits.
    pub config: Arc<ServiceConfig>,
}

impl Services {
    /// Build the facade from its parts.
    pub fn new(
        stores: Stores,
        infra: Infra,
        ids: Arc<dyn IdGenerator>,
        tokens: Arc<TokenManager>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            stores,
            infra,
            ids,
            tokens,
            clock,
            config: Arc::new(ServiceConfig::default()),
        }
    }

    /// Override the service limits (builder style).
    pub fn with_config(mut self, config: ServiceConfig) -> Self {
        self.config = Arc::new(config);
        self
    }

    /// Generate a fresh identifier.
    pub(crate) fn new_id(&self) -> i64 {
        self.ids.next_id()
    }

    /// The current time.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// Clamp a requested page size into the configured bounds.
    pub(crate) fn page_size(&self, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.config.default_page_size)
            .clamp(1, self.config.max_page_size)
    }
}

/// Validate a message payload and return its normalized text.
pub(crate) fn normalize_text(input: &str, max: usize) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(crate::error::Error::Invalid(
            "message must not be empty".into(),
        ));
    }
    if trimmed.chars().count() > max {
        return Err(crate::error::Error::Invalid(format!(
            "message exceeds {max} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Normalize an optional user-provided text field. Empty input clears it.
pub(crate) fn normalize_optional_text(input: Option<String>, max: usize) -> Result<Option<String>> {
    let Some(raw) = input else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max {
        return Err(crate::error::Error::Invalid(format!(
            "value exceeds {max} characters"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

/// Normalize an avatar reference. Empty input clears it.
///
/// Avatars are either absolute http(s) URLs or inline `data:image/...` URLs.
/// The latter are produced by the web client after downscaling the upload.
pub(crate) fn normalize_avatar(input: Option<String>) -> Result<Option<String>> {
    const MAX_AVATAR_CHARS: usize = 700_000;
    let Some(raw) = input else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_AVATAR_CHARS {
        return Err(crate::error::Error::Invalid(
            "avatar is too large (max 512 KiB)".into(),
        ));
    }
    let allowed = trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("data:image/")
        || trimmed.starts_with('/');
    if !allowed {
        return Err(crate::error::Error::Invalid(
            "avatar must be an http(s), data:image, or absolute-path URL".into(),
        ));
    }
    Ok(Some(trimmed.to_string()))
}
