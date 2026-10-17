//! Storage and infrastructure adapters for ChatBox.
//!
//! The crate exposes interchangeable implementations of the ports defined in
//! `chatbox-core`:
//!
//! - [`memory`] — process-local state for tests and development.
//! - [`mysql`] — the production relational backend.
//! - [`redis`] — the production cache, presence, registry, rate-limit, and
//!   fan-out transport.
//! - [`object`] — object storage adapters (local filesystem).

pub mod backend;
pub mod memory;
pub mod mysql;
pub mod object;
pub mod redis;

pub use backend::{Backend, BackendConfig};
