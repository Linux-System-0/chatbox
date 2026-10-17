//! HTTP-level test: a server plugin rewrite reaches persistence.

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chatbox_api::AppState;
use chatbox_core::infra::Infra;
use chatbox_core::model::{MessageKind, Platform};
use chatbox_core::port::Stores;
use chatbox_core::services::{AuthTokens, RegisterInput, Services};
use chatbox_core::tokens::TokenConfig;
use chatbox_core::{Result as CoreResult, Snowflake, SystemClock, TokenManager};
use chatbox_plugin::{
    HookContext, HookOutcome, MessagePatch, PLUGIN_API_VERSION, PluginHost, PluginInfo,
    ServerPlugin,
};
use chatbox_storage::memory::MemoryStore;
use serde_json::{Value, json};
use tower::ServiceExt;

struct Redactor;

#[async_trait]
impl ServerPlugin for Redactor {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "test-redactor".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            description: "replace the body".into(),
            builtin: false,
        }
    }

    async fn on_message_before_persist(&self, _ctx: &HookContext) -> CoreResult<HookOutcome> {
        Ok(HookOutcome::Rewrite(MessagePatch {
            content: Some("[redacted]".into()),
            kind: None,
        }))
    }
}

struct Rejecter;

#[async_trait]
impl ServerPlugin for Rejecter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "test-rejecter".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            description: "reject everything".into(),
            builtin: false,
        }
    }

    async fn on_message_before_persist(&self, _ctx: &HookContext) -> CoreResult<HookOutcome> {
        Ok(HookOutcome::Reject("blocked by policy".into()))
    }
}

fn harness() -> (Services, Arc<PluginHost>) {
    let memory = Arc::new(MemoryStore::new());
    let stores = Stores {
        users: memory.clone(),
        sessions: memory.clone(),
        devices: memory.clone(),
        conversations: memory.clone(),
        messages: memory.clone(),
        contacts: memory.clone(),
        friend_requests: memory.clone(),
        groups: memory.clone(),
        blocks: memory.clone(),
        attachments: memory.clone(),
        outbox: memory.clone(),
        plugins: memory.clone(),
        plugin_kv: memory.clone(),
    };
    let infra = Infra {
        cache: memory.clone(),
        presence: memory.clone(),
        registry: memory.clone(),
        bus: memory.clone(),
        rate_limiter: memory.clone(),
        objects: memory,
    };
    let tokens = Arc::new(TokenManager::new(TokenConfig {
        secret: "test-secret".into(),
        ..Default::default()
    }));
    let services = Services::new(
        stores,
        infra,
        Arc::new(Snowflake::new(1)),
        tokens,
        Arc::new(SystemClock),
    );
    let plugins = Arc::new(PluginHost::new(services.stores.plugins.clone()));
    (services, plugins)
}

async fn register(services: &Services, user_id: &str) -> AuthTokens {
    let (_, tokens) = services
        .register(RegisterInput {
            user_id: user_id.into(),
            password: "password123".into(),
            display_name: None,
            device_name: "test".into(),
            platform: Platform::Web,
        })
        .await
        .expect("register");
    tokens
}

#[tokio::test]
async fn rewrites_before_persisting_over_http() {
    let (services, plugins) = harness();
    plugins.register(Arc::new(Redactor)).await;
    plugins.enable("test-redactor").await.unwrap();

    let alice = register(&services, "alice").await;
    register(&services, "bob").await;
    let ctx = services.authenticate(&alice.access_token).await.unwrap();
    let conversation = services.get_or_create_direct(&ctx, "bob").await.unwrap();

    let state = AppState {
        services: services.clone(),
        plugins: plugins.clone(),
        node_id: "test".into(),
        admin_user_ids: Arc::default(),
    };
    let body = json!({
        "client_msg_id": "m1",
        "kind": "text",
        "content": "my email is alice@example.com",
    });
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/conversations/{}/messages",
            conversation.id
        ))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", alice.access_token))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = chatbox_api::router(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["message"]["content"], "[redacted]");

    // The stored message, not just the response, carries the rewrite.
    let stored = services
        .stores
        .messages
        .get(value["message"]["id"].as_i64().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.content.as_deref(), Some("[redacted]"));
    assert_eq!(stored.kind, MessageKind::Text);

    // A rejection surfaces as 403 with the plugin's reason.
    plugins.register(Arc::new(Rejecter)).await;
    plugins.enable("test-rejecter").await.unwrap();
    let state = AppState {
        services: services.clone(),
        plugins,
        node_id: "test".into(),
        admin_user_ids: Arc::default(),
    };
    let body = json!({ "client_msg_id": "m2", "kind": "text", "content": "again" });
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/conversations/{}/messages",
            conversation.id
        ))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", alice.access_token))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = chatbox_api::router(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"]["code"], "forbidden");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("blocked by policy")
    );
}
