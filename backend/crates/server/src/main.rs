//! ChatBox server composition root.
//!
//! Builds the storage backend, domain services, plugin host, realtime gateway,
//! and background workers, then serves the combined HTTP/WebSocket router.

mod config;

use std::sync::Arc;

use anyhow::{Context, Result};
use chatbox_api::{router as api_router, AppState};
use chatbox_core::services::ServiceConfig;
use chatbox_core::tokens::TokenConfig;
use chatbox_core::{Services, Snowflake, SystemClock, TokenManager};
use chatbox_plugin::{AuditLogPlugin, MessageGuardPlugin, PluginHost};
use chatbox_realtime::{Gateway, Hub};
use chatbox_storage::Backend;
use chatbox_workers::{Fanout, OutboxRelay};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env()?;
    let backend = Backend::connect(config.backend())
        .await
        .context("failed to initialise storage")?;
    tracing::info!(backend = backend.kind, "storage backend ready");

    let tokens = Arc::new(TokenManager::new(TokenConfig {
        secret: config.jwt_secret.clone(),
        ..Default::default()
    }));
    let services = Services::new(
        backend.stores.clone(),
        backend.infra.clone(),
        Arc::new(Snowflake::new(config.node_number)),
        tokens,
        Arc::new(SystemClock),
    )
    .with_config(ServiceConfig {
        fanout_write_max_members: config.fanout_write_max_members,
        ..Default::default()
    });

    // Plugin host.
    let plugins = Arc::new(PluginHost::new(backend.stores.plugins.clone()));
    plugins.register(Arc::new(AuditLogPlugin)).await;
    plugins
        .register(Arc::new(MessageGuardPlugin::new(
            config.guard_words.clone(),
        )))
        .await;
    match plugins.discover_dir(&config.plugin_dir).await {
        Ok(count) => {
            tracing::info!(count, dir = %config.plugin_dir.display(), "plugin discovery complete")
        }
        Err(e) => tracing::warn!(error = %e, "plugin discovery failed"),
    }
    plugins
        .bootstrap()
        .await
        .context("plugin bootstrap failed")?;
    plugins.register_builtin_client_manifests().await;

    // Realtime hub and background relay.
    let hub = Hub::new(config.node_id.clone());
    hub.spawn_delivery_listener(backend.infra.bus.clone());
    let fanout = Fanout::new(
        backend.stores.clone(),
        backend.infra.clone(),
        config.node_id.clone(),
        config.fanout_write_max_members,
    );
    let relay = OutboxRelay::new(backend.stores.clone(), fanout);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(relay.run(shutdown_rx));

    // Routers.
    let state = AppState {
        services: services.clone(),
        plugins: plugins.clone(),
        node_id: config.node_id.clone(),
        admin_user_ids: Arc::new(config.admin_user_ids.clone()),
    };
    let gateway = Gateway {
        services,
        plugins,
        hub,
    };
    let app = api_router(state).merge(gateway.router());

    let listener = tokio::net::TcpListener::bind(&config.http_bind)
        .await
        .with_context(|| format!("failed to bind {}", config.http_bind))?;
    tracing::info!(addr = %config.http_bind, "chatbox listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    let _ = shutdown_tx.send(true);
    tracing::info!("shutdown complete");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
