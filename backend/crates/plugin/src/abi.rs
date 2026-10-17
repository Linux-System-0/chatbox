//! C ABI shared by the host and dynamically loaded server plugins.
//!
//! The ABI is versioned. The host loads only plugins whose declared version
//! equals [`PLUGIN_API_VERSION`]; older plugins are skipped with a warning.

use std::os::raw::{c_char, c_void};

/// The exported symbol a plugin library must define.
pub const PLUGIN_INIT_SYMBOL: &[u8] = b"chatbox_plugin_init\0";

/// The plugin API version this host implements.
///
/// Version 2 replaced the integer hook result with a JSON document and added
/// the host callback table.
pub const PLUGIN_API_VERSION: u32 = 2;

/// The host callback table version.
pub const HOST_API_VERSION: u32 = 1;

/// Hooks receive a NUL-terminated JSON context and the host table, and return
/// a NUL-terminated JSON result owned by the plugin (or `NULL` for "no
/// opinion").
///
/// The `C-unwind` ABI lets a Rust plugin panic unwind into the host, which
/// catches and isolates it instead of aborting the process.
pub type HookFn = unsafe extern "C-unwind" fn(*const c_char, *const HostApi) -> *mut c_char;

/// Host callbacks available to a plugin while one of its hooks runs.
#[repr(C)]
pub struct HostApi {
    /// ABI version of this table; must equal [`HOST_API_VERSION`].
    pub api_version: u32,
    /// Opaque host context; only valid for the duration of the hook call.
    pub ctx: *mut c_void,
    /// Execute a host call. `params` is a JSON document (or `NULL`). Returns a
    /// JSON envelope owned by the host, or `NULL` on failure.
    pub call: Option<
        unsafe extern "C-unwind" fn(
            ctx: *mut c_void,
            method: *const c_char,
            params: *const c_char,
        ) -> *mut c_char,
    >,
    /// Release a string returned by [`HostApi::call`].
    pub free_string: Option<unsafe extern "C-unwind" fn(*mut c_char)>,
}

/// Function table returned by a plugin's init symbol.
///
/// Every string field is a NUL-terminated UTF-8 `char*` owned by the plugin and
/// valid for the lifetime of the loaded library. Hook results are allocated by
/// the plugin and released with [`PluginVTable::free_string`].
#[repr(C)]
pub struct PluginVTable {
    /// ABI version the plugin was built against.
    pub api_version: u32,
    /// Plugin name.
    pub name: *const c_char,
    /// Plugin version.
    pub version: *const c_char,
    /// Human description.
    pub description: *const c_char,
    /// Called before a message is persisted.
    pub on_message_before_persist: Option<HookFn>,
    /// Called after a message is persisted.
    pub on_message_after_persist: Option<HookFn>,
    /// Called after a user registers.
    pub on_user_register: Option<HookFn>,
    /// Called after a group is created.
    pub on_group_created: Option<HookFn>,
    /// Release a string returned by one of the hook functions.
    pub free_string: Option<unsafe extern "C-unwind" fn(*mut c_char)>,
}

/// Signature of the plugin init symbol.
pub type PluginInitFn = unsafe extern "C-unwind" fn(u32, *mut PluginVTable, *const HostApi) -> i32;
