//! Dynamically loaded server plugins.
//!
//! A plugin is a shared library exporting `chatbox_plugin_init`, which fills a
//! [`PluginVTable`]. The host keeps the library mapped for as long as the
//! plugin is registered. Hooks are isolated: a panic or error is reported and
//! does not take down the host.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chatbox_core::error::{Error, Result};

use crate::abi::{PLUGIN_API_VERSION, PLUGIN_INIT_SYMBOL, PluginInitFn, PluginVTable};
use crate::{HookContext, HookOutcome, PluginInfo, ServerPlugin};

/// A plugin loaded from a shared library.
pub struct LoadedPlugin {
    // The library must outlive every function pointer stored below.
    _library: Arc<libloading::Library>,
    info: PluginInfo,
    before: Option<extern "C" fn(*const c_char) -> i32>,
    after: Option<extern "C" fn(*const c_char) -> i32>,
    user: Option<extern "C" fn(*const c_char) -> i32>,
    group: Option<extern "C" fn(*const c_char) -> i32>,
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
    pub unsafe fn load(path: &Path, manifest_info: PluginInfo) -> Result<Self> {
        let library = Arc::new(unsafe { libloading::Library::new(path) }.map_err(Error::storage)?);
        let init: libloading::Symbol<PluginInitFn> = unsafe { library.get(PLUGIN_INIT_SYMBOL) }
            .map_err(|e| Error::Invalid(format!("plugin missing init symbol: {e}")))?;
        let mut vtable = PluginVTable {
            api_version: 0,
            name: std::ptr::null(),
            version: std::ptr::null(),
            description: std::ptr::null(),
            on_message_before_persist: None,
            on_message_after_persist: None,
            on_user_register: None,
            on_group_created: None,
        };
        let rc = unsafe { init(PLUGIN_API_VERSION, &mut vtable) };
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
        })
    }

    fn call_hook(
        hook: Option<extern "C" fn(*const c_char) -> i32>,
        ctx: &HookContext,
    ) -> Result<i32> {
        let Some(hook) = hook else {
            return Ok(0);
        };
        let payload = serde_json::to_string(ctx).map_err(Error::storage)?;
        let payload = CString::new(payload).map_err(Error::storage)?;
        Ok(hook(payload.as_ptr()))
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

    async fn on_message_before_persist(&self, ctx: &HookContext) -> Result<HookOutcome> {
        match Self::call_hook(self.before, ctx)? {
            0 => Ok(HookOutcome::Allow),
            code => Ok(HookOutcome::Reject(format!(
                "plugin rejected message ({code})"
            ))),
        }
    }

    async fn on_message_after_persist(&self, ctx: &HookContext) -> Result<()> {
        Self::call_hook(self.after, ctx)?;
        Ok(())
    }

    async fn on_user_register(&self, ctx: &HookContext) -> Result<()> {
        Self::call_hook(self.user, ctx)?;
        Ok(())
    }

    async fn on_group_created(&self, ctx: &HookContext) -> Result<()> {
        Self::call_hook(self.group, ctx)?;
        Ok(())
    }
}
