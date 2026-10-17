//! ChatBox HTTP/REST control plane.
//!
//! Handlers are thin: they extract and validate transport concerns, delegate
//! to [`chatbox_core::services::Services`], and translate domain errors into
//! HTTP responses. See `docs/api.md` for the contract.

mod error;
mod handlers;

use std::collections::HashSet;
use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, patch, post, put};
use chatbox_core::services::Services;
use chatbox_plugin::PluginHost;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use error::{ApiError, ApiResult};

/// Shared state for every HTTP handler.
#[derive(Clone)]
pub struct AppState {
    /// Domain services.
    pub services: Services,
    /// Server plugin host.
    pub plugins: Arc<PluginHost>,
    /// Node identifier (used in logs and diagnostics).
    pub node_id: String,
    /// Custom user ids allowed to manage server plugins. When empty, any
    /// authenticated user may do so (development default).
    pub admin_user_ids: Arc<HashSet<String>>,
}

impl AppState {
    /// Whether the authenticated user may manage server plugins.
    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_user_ids.is_empty() || self.admin_user_ids.contains(user_id)
    }
}

/// Build the control-plane router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health/live", get(handlers::health_live))
        .route("/api/v1/health/ready", get(handlers::health_ready))
        .route("/api/v1/auth/register", post(handlers::register))
        .route("/api/v1/auth/login", post(handlers::login))
        .route("/api/v1/auth/refresh", post(handlers::refresh))
        .route("/api/v1/auth/logout", post(handlers::logout))
        .route("/api/v1/auth/sessions", get(handlers::sessions))
        .route(
            "/api/v1/users/me",
            get(handlers::me).patch(handlers::update_me),
        )
        .route("/api/v1/users/{user_id}", get(handlers::get_user))
        .route("/api/v1/presence", get(handlers::presence))
        .route("/api/v1/devices", get(handlers::list_devices))
        .route("/api/v1/devices/{id}", delete(handlers::delete_device))
        .route("/api/v1/contacts", get(handlers::list_contacts))
        .route(
            "/api/v1/contacts/requests",
            get(handlers::list_requests).post(handlers::send_request),
        )
        .route(
            "/api/v1/contacts/requests/{id}/accept",
            post(handlers::accept_request),
        )
        .route(
            "/api/v1/contacts/requests/{id}/reject",
            post(handlers::reject_request),
        )
        .route(
            "/api/v1/contacts/{user_id}",
            patch(handlers::set_remark).delete(handlers::remove_contact),
        )
        .route("/api/v1/conversations", get(handlers::list_conversations))
        .route(
            "/api/v1/conversations/direct",
            post(handlers::create_direct),
        )
        .route(
            "/api/v1/conversations/{id}/messages",
            get(handlers::list_messages).post(handlers::send_message),
        )
        .route("/api/v1/conversations/{id}/read", post(handlers::mark_read))
        .route("/api/v1/groups", post(handlers::create_group))
        .route(
            "/api/v1/groups/{id}",
            get(handlers::get_group)
                .patch(handlers::rename_group)
                .delete(handlers::dissolve_group),
        )
        .route("/api/v1/groups/{id}/members", post(handlers::add_members))
        .route(
            "/api/v1/groups/{id}/members/{user_id}",
            delete(handlers::kick_member),
        )
        .route(
            "/api/v1/groups/{id}/members/{user_id}/mute",
            post(handlers::mute_member),
        )
        .route(
            "/api/v1/groups/{id}/members/{user_id}/role",
            post(handlers::set_member_role),
        )
        .route(
            "/api/v1/groups/{id}/remark",
            patch(handlers::set_group_remark),
        )
        .route("/api/v1/groups/{id}/transfer", post(handlers::transfer))
        .route("/api/v1/groups/{id}/leave", post(handlers::leave_group))
        .route("/api/v1/blocks", get(handlers::list_blocks))
        .route(
            "/api/v1/blocks/{user_id}",
            post(handlers::block_user).delete(handlers::unblock_user),
        )
        .route("/api/v1/search", get(handlers::search))
        .route("/api/v1/files/presign", post(handlers::presign))
        .route("/api/v1/files/upload/{id}", put(handlers::upload))
        .route("/api/v1/files/{id}", get(handlers::get_file))
        .route("/api/v1/files/{id}/complete", post(handlers::complete_file))
        .route("/api/v1/files/{id}/content", get(handlers::file_content))
        .route("/api/v1/plugins/client", get(handlers::client_plugins))
        .route("/api/v1/plugins/server", get(handlers::server_plugins))
        .route(
            "/api/v1/plugins/server/{name}/enable",
            post(handlers::enable_plugin),
        )
        .route(
            "/api/v1/plugins/server/{name}/disable",
            post(handlers::disable_plugin),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
