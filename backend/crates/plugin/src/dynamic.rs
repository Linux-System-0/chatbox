//! Dynamically loaded server plugins.
//!
//! A plugin is a shared library exporting `chatbox_plugin_init`, which fills a
//! [`PluginVTable`] and receives the [`HostApi`] callback table. The host keeps
//! the library mapped for as long as the plugin is registered.
//!
//! Hooks run on the blocking pool and are wrapped in `catch_unwind`, so a
//! panicking or failing plugin is reported and skipped instead of taking down
//! the host. A timed-out call keeps its blocking thread until the plugin
//! returns; the host simply stops waiting for it.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};
use serde_json::Value;

use crate::abi::{
    HOST_API_VERSION, HostApi, PLUGIN_API_VERSION, PLUGIN_INIT_SYMBOL, PluginInitFn, PluginVTable,
};
use crate::host::ServerHost;
use crate::{HookContext, PluginInfo, ServerPlugin, parse_before_result};

/// Upper bound on one hook result document.
pub const MAX_HOOK_RESULT_BYTES: usize = 1024 * 1024;

/// State behind the opaque host context passed to a plugin.
struct HostCallContext {
    host: Arc<ServerHost>,
    plugin: String,
}

/// A plugin loaded from a shared library.
pub struct LoadedPlugin {
    // The library must outlive every function pointer stored below.
    _library: Arc<libloading::Library>,
    info: PluginInfo,
    before: Option<crate::abi::HookFn>,
    after: Option<crate::abi::HookFn>,
    user: Option<crate::abi::HookFn>,
    group: Option<crate::abi::HookFn>,
    free_string: Option<unsafe extern "C-unwind" fn(*mut c_char)>,
    // Boxed so the pointer handed to plugins stays valid if `Self` moves.
    host_ctx: Option<Box<HostCallContext>>,
}

// SAFETY: the vtable function pointers are `extern "C"` functions with no
// captured state, and the plugin contract requires them to be thread-safe.
// The library stays mapped for the lifetime of this value.
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}

impl LoadedPlugin {
    /// Load a plugin from `path`. `manifest` supplies the declared metadata.
    ///
    /// # Safety
    /// The caller must ensure `path` points to a valid ChatBox plugin built
    /// for the ABI in [`crate::abi`].
    pub unsafe fn load(
        path: &Path,
        manifest_info: PluginInfo,
        host: Option<Arc<ServerHost>>,
    ) -> Result<Self> {
        let library = Arc::new(unsafe { libloading::Library::new(path) }.map_err(Error::storage)?);
        let init: libloading::Symbol<PluginInitFn> = unsafe { library.get(PLUGIN_INIT_SYMBOL) }
            .map_err(|e| Error::Invalid(format!("plugin missing init symbol: {e}")))?;

        let host_call_ctx = host.map(|host| {
            Box::new(HostCallContext {
                host,
                plugin: manifest_info.name.clone(),
            })
        });
        let host_api = host_api(
            host_call_ctx
                .as_deref()
                .map(|ctx| ctx as *const HostCallContext as *mut c_void),
        );

        let mut vtable = PluginVTable {
            api_version: 0,
            name: ptr::null(),
            version: ptr::null(),
            description: ptr::null(),
            on_message_before_persist: None,
            on_message_after_persist: None,
            on_user_register: None,
            on_group_created: None,
            free_string: None,
        };
        let rc = unsafe { init(PLUGIN_API_VERSION, &mut vtable, &host_api) };
        if rc != 0 {
            return Err(Error::Invalid(format!("plugin init failed with code {rc}")));
        }
        if vtable.api_version != PLUGIN_API_VERSION {
            return Err(Error::Invalid(format!(
                "plugin ABI version {} is not supported (expected {PLUGIN_API_VERSION})",
                vtable.api_version
            )));
        }
        let name = cstr(vtable.name).unwrap_or(manifest_info.name);
        let version = cstr(vtable.version).unwrap_or(manifest_info.version);
        let description = cstr(vtable.description).unwrap_or(manifest_info.description);
        let host_ctx = host_call_ctx.map(|mut ctx| {
            ctx.plugin = name.clone();
            ctx
        });
        Ok(Self {
            _library: library,
            info: PluginInfo {
                name,
                version,
                api_version: vtable.api_version,
                description,
                builtin: false,
            },
            before: vtable.on_message_before_persist,
            after: vtable.on_message_after_persist,
            user: vtable.on_user_register,
            group: vtable.on_group_created,
            free_string: vtable.free_string,
            host_ctx,
        })
    }

    /// Run one hook on the blocking pool and parse its JSON result.
    async fn call_hook(
        &self,
        hook: crate::abi::HookFn,
        ctx: &HookContext,
    ) -> Result<Option<Value>> {
        let payload = serde_json::to_string(ctx).map_err(Error::storage)?;
        // Raw pointers are not `Send`; carry the address and rebuild it on the
        // blocking thread. The boxed context outlives the call.
        let host_ctx = self
            .host_ctx
            .as_deref()
            .map(|ctx| ctx as *const HostCallContext as usize);
        let free_string = self.free_string;
        tokio::task::spawn_blocking(move || {
            let payload =
                CString::new(payload).map_err(|_| Error::Invalid("hook context has NUL".into()))?;
            let host_ctx = host_ctx.map(|address| address as *mut c_void);
            let api = host_api(host_ctx);
            let api_ptr = if host_ctx.is_some() {
                &api as *const HostApi
            } else {
                ptr::null()
            };
            // `C-unwind` lets a Rust panic unwind here instead of aborting.
            let out = std::panic::catch_unwind(AssertUnwindSafe(|| unsafe {
                hook(payload.as_ptr(), api_ptr)
            }))
            .map_err(|_| Error::Internal("plugin panicked".into()))?;
            if out.is_null() {
                return Ok(None);
            }
            // SAFETY: the plugin contract requires a NUL-terminated string.
            let text = unsafe { CStr::from_ptr(out) }
                .to_str()
                .map_err(|_| Error::Invalid("plugin result is not UTF-8".into()))?
                .to_string();
            if let Some(free) = free_string {
                unsafe { free(out) };
            }
            if text.len() > MAX_HOOK_RESULT_BYTES {
                return Err(Error::Invalid(format!(
                    "plugin result exceeds {MAX_HOOK_RESULT_BYTES} bytes"
                )));
            }
            let value = serde_json::from_str(&text)
                .map_err(|e| Error::Invalid(format!("invalid plugin result: {e}")))?;
            Ok(Some(value))
        })
        .await
        .map_err(|e| Error::Internal(format!("plugin hook task failed: {e}")))?
    }
}

fn host_api(host_ctx: Option<*mut c_void>) -> HostApi {
    HostApi {
        api_version: HOST_API_VERSION,
        ctx: host_ctx.unwrap_or(ptr::null_mut()),
        call: Some(host_call),
        free_string: Some(host_free_string),
    }
}

unsafe extern "C-unwind" fn host_call(
    ctx: *mut c_void,
    method: *const c_char,
    params: *const c_char,
) -> *mut c_char {
    if ctx.is_null() || method.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the host passes the boxed context owned by `LoadedPlugin`.
    let context = unsafe { &*(ctx as *const HostCallContext) };
    let Ok(method) = (unsafe { CStr::from_ptr(method) }).to_str() else {
        return ptr::null_mut();
    };
    let params = if params.is_null() {
        Value::Null
    } else {
        match unsafe { CStr::from_ptr(params) }.to_str() {
            Ok(text) => serde_json::from_str(text).unwrap_or(Value::Null),
            Err(_) => return ptr::null_mut(),
        }
    };
    let response = match context.host.call_blocking(&context.plugin, method, params) {
        Ok(result) => serde_json::json!({ "ok": true, "result": result }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    };
    CString::new(response.to_string())
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

unsafe extern "C-unwind" fn host_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: the pointer came from `CString::into_raw` in `host_call`.
        drop(unsafe { CString::from_raw(ptr) });
    }
}

fn cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the plugin contract requires a valid NUL-terminated string.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_string)
}

#[async_trait]
impl ServerPlugin for LoadedPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<crate::HookOutcome> {
        let Some(hook) = self.before else {
            return Ok(crate::HookOutcome::Allow);
        };
        match self.call_hook(hook, ctx).await? {
            Some(value) => parse_before_result(&value),
            None => Ok(crate::HookOutcome::Allow),
        }
    }

    async fn on_message_after_persist(&self, ctx: &HookContext) -> Result<()> {
        if let Some(hook) = self.after {
            let _ = self.call_hook(hook, ctx).await?;
        }
        Ok(())
    }

    async fn on_user_register(&self, ctx: &HookContext) -> Result<()> {
        if let Some(hook) = self.user {
            let _ = self.call_hook(hook, ctx).await?;
        }
        Ok(())
    }

    async fn on_group_created(&self, ctx: &HookContext) -> Result<()> {
        if let Some(hook) = self.group {
            let _ = self.call_hook(hook, ctx).await?;
        }
        Ok(())
    }
}
