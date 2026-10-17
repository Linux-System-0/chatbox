//! Domain error type shared by services and adapters.

/// The canonical error type for the ChatBox domain.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested entity does not exist (or is not visible to the caller).
    #[error("not found")]
    NotFound,

    /// The operation violates a uniqueness or state constraint.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The caller supplied invalid input.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// Authentication is missing or invalid.
    #[error("unauthorized")]
    Unauthorized,

    /// The caller is authenticated but not permitted.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// A rate limit was exceeded.
    #[error("rate limited")]
    RateLimited,

    /// An infrastructure dependency failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// An unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Build a storage error from any displayable cause.
    pub fn storage<E: std::fmt::Display>(e: E) -> Self {
        Error::Storage(e.to_string())
    }

    /// The stable machine-readable code used in API error envelopes.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound => "not_found",
            Error::Conflict(_) => "conflict",
            Error::Invalid(_) => "invalid_input",
            Error::Unauthorized => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::RateLimited => "rate_limited",
            Error::Storage(_) => "storage_error",
            Error::Internal(_) => "internal_error",
        }
    }
}

/// Convenience alias used throughout the domain.
pub type Result<T> = std::result::Result<T, Error>;
