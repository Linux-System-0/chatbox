//! ChatBox core domain.
//!
//! This crate owns the models, the port traits (repository and infrastructure
//! interfaces), and the business services. It has no dependency on a concrete
//! database or transport so the domain can be tested with in-memory adapters.

pub mod clock;
pub mod error;
pub mod ids;
pub mod infra;
pub mod model;
pub mod password;
pub mod port;
pub mod protocol;
pub mod services;
pub mod tokens;

pub use clock::{Clock, SystemClock};
pub use error::{Error, Result};
pub use ids::{IdGenerator, Snowflake};
pub use infra::Infra;
pub use model::*;
pub use port::Stores;
pub use services::Services;
pub use tokens::TokenManager;
