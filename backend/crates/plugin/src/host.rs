//! Host API served to dynamically loaded server plugins.
//!
//! A plugin calls back into the server through one JSON entry point,
//! [`HostApi::call`](../abi/struct.HostApi.html). The calls are dispatched from
//! a dedicated thread that owns a current-thread Tokio runtime, because the
//! caller is a blocking plugin hook spawned on the blocking pool and must not
//! start a nested runtime.
//!
//! The API is deliberately read-only: no method can send messages or mutate
//! domain state, so a hook can never re-enter the delivery pipeline.

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::thread;

use chatbox_core::Clock;
use chatbox_core::error::{Error, Result};
use chatbox_core::port::Stores;
use serde_json::{Value, json};

/// Maximum key length accepted by the KV methods.
const MAX_KEY_BYTES: usize = 255;
/// Maximum value size accepted by the KV methods.
const MAX_VALUE_BYTES: usize = 64 * 1024;
/// Maximum number of entries returned by `kv.list`.
const MAX_LIST_ENTRIES: usize = 1000;
/// Maximum TTL accepted by `kv.set` (one year).
const MAX_TTL_SECS: i64 = 366 * 24 * 60 * 60;

struct Job {
    plugin: String,
    method: String,
    params: Value,
    reply: std_mpsc::SyncSender<Result<Value>>,
}

/// The host API implementation shared by every dynamic plugin.
///
/// Construction starts one dispatcher thread; it stops when the last handle is
/// dropped. Cloning is cheap and shares the same thread.
#[derive(Clone)]
pub struct ServerHost {
    tx: std_mpsc::Sender<Job>,
}

impl ServerHost {
    /// Start the dispatcher thread for the given stores and clock.
    pub fn new(stores: Stores, clock: Arc<dyn Clock>) -> Self {
        let (tx, rx) = std_mpsc::channel::<Job>();
        let spawned = thread::Builder::new()
            .name("chatbox-plugin-host".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to start the plugin host runtime");
                        return;
                    }
                };
                while let Ok(job) = rx.recv() {
                    let result = runtime.block_on(handle(
                        &stores,
                        &clock,
                        &job.plugin,
                        &job.method,
                        job.params,
                    ));
                    // A timed-out caller drops its receiver; that is expected.
                    let _ = job.reply.send(result);
                }
            });
        if let Err(e) = spawned {
            tracing::error!(error = %e, "failed to start the plugin host thread");
        }
        Self { tx }
    }

    /// Execute one host call synchronously.
    ///
    /// Only call this from a blocking plugin hook: it blocks the calling thread
    /// until the dispatcher replies.
    pub fn call_blocking(&self, plugin: &str, method: &str, params: Value) -> Result<Value> {
        let (reply, rx) = std_mpsc::sync_channel(1);
        self.tx
            .send(Job {
                plugin: plugin.to_string(),
                method: method.to_string(),
                params,
                reply,
            })
            .map_err(|_| Error::Internal("plugin host is unavailable".into()))?;
        rx.recv()
            .map_err(|_| Error::Internal("plugin host dropped the call".into()))?
    }
}

async fn handle(
    stores: &Stores,
    clock: &Arc<dyn Clock>,
    plugin: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    match method {
        "log" => {
            let level = params
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("info");
            let message = string_param(&params, "message")?;
            match level {
                "error" => tracing::error!(target: "chatbox::plugin", plugin, "{message}"),
                "warn" => tracing::warn!(target: "chatbox::plugin", plugin, "{message}"),
                "debug" => tracing::debug!(target: "chatbox::plugin", plugin, "{message}"),
                "trace" => tracing::trace!(target: "chatbox::plugin", plugin, "{message}"),
                _ => tracing::info!(target: "chatbox::plugin", plugin, "{message}"),
            }
            Ok(Value::Null)
        }
        "now" => Ok(Value::String(clock.now().to_rfc3339())),
        "user.get" => {
            let user_id = string_param(&params, "user_id")?;
            let Some(user) = stores.users.get_by_user_id(user_id).await? else {
                return Ok(Value::Null);
            };
            Ok(json!({
                "id": user.id,
                "user_id": user.user_id,
                "display_name": user.display_name,
                "created_at": user.created_at,
            }))
        }
        "conversation.get" => {
            let conversation_id = integer_param(&params, "conversation_id")?;
            let Some(conversation) = stores.conversations.get(conversation_id).await? else {
                return Ok(Value::Null);
            };
            let members = stores
                .conversations
                .list_members(conversation_id)
                .await?
                .into_iter()
                .map(|member| member.user_id)
                .collect::<Vec<_>>();
            Ok(json!({
                "id": conversation.id,
                "kind": conversation.kind,
                "created_at": conversation.created_at,
                "members": members,
            }))
        }
        "kv.get" => {
            let key = kv_key(&params)?;
            let value = stores.plugin_kv.get(plugin, &key).await?;
            Ok(value.map(Value::String).unwrap_or(Value::Null))
        }
        "kv.set" => {
            let key = kv_key(&params)?;
            let value = string_param(&params, "value")?;
            if value.len() > MAX_VALUE_BYTES {
                return Err(Error::Invalid(format!(
                    "plugin value exceeds {MAX_VALUE_BYTES} bytes"
                )));
            }
            let ttl = params.get("ttl_secs").and_then(Value::as_i64);
            let at = clock.now();
            let expires_at = match ttl {
                None => None,
                Some(ttl) if (0..=MAX_TTL_SECS).contains(&ttl) => {
                    Some(at + chrono::Duration::seconds(ttl))
                }
                Some(_) => {
                    return Err(Error::Invalid(format!(
                        "ttl_secs must be between 0 and {MAX_TTL_SECS}"
                    )));
                }
            };
            stores
                .plugin_kv
                .set(plugin, &key, value, expires_at, at)
                .await?;
            Ok(Value::Null)
        }
        "kv.delete" => {
            let key = kv_key(&params)?;
            stores.plugin_kv.delete(plugin, &key).await?;
            Ok(Value::Null)
        }
        "kv.list" => {
            let prefix = params.get("prefix").and_then(Value::as_str).unwrap_or("");
            if prefix.len() > MAX_KEY_BYTES {
                return Err(Error::Invalid("prefix is too long".into()));
            }
            let rows = stores.plugin_kv.list(plugin, prefix).await?;
            Ok(Value::Array(
                rows.into_iter()
                    .take(MAX_LIST_ENTRIES)
                    .map(|(key, value)| json!({ "key": key, "value": value }))
                    .collect(),
            ))
        }
        other => Err(Error::Invalid(format!("unknown host method '{other}'"))),
    }
}

fn string_param<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("missing string parameter '{key}'")))
}

fn integer_param(params: &Value, key: &str) -> Result<i64> {
    params
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::Invalid(format!("missing integer parameter '{key}'")))
}

fn kv_key(params: &Value) -> Result<String> {
    let key = string_param(params, "key")?;
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(Error::Invalid(format!(
            "kv key must be 1..={MAX_KEY_BYTES} bytes"
        )));
    }
    Ok(key.to_string())
}
