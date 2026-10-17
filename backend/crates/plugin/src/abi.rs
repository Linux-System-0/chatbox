//! C ABI shared by the host and dynamically loaded server plugins.

use std::os::raw::c_char;

/// The exported symbol a plugin library must define.
pub const PLUGIN_INIT_SYMBOL: &[u8] = b"chatbox_plugin_init\0";

/// The plugin API version this host implements.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Function table returned by a plugin's init symbol.
///
/// Every string is a NUL-terminated UTF-8 `char*` owned by the plugin and
/// valid for the lifetime of the loaded library. Hook functions receive a
/// NUL-terminated JSON document and return `0` for success; a non-zero return
/// from `on_message_before_persist` rejects the message.
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
    pub on_message_before_persist: Option<extern "C" fn(*const c_char) -> i32>,
    /// Called after a message is persisted.
    pub on_message_after_persist: Option<extern "C" fn(*const c_char) -> i32>,
    /// Called after a user registers.
    pub on_user_register: Option<extern "C" fn(*const c_char) -> i32>,
    /// Called after a group is created.
    pub on_group_created: Option<extern "C" fn(*const c_char) -> i32>,
}

/// Signature of the plugin init symbol.
pub type PluginInitFn = unsafe extern "C" fn(u32, *mut PluginVTable) -> i32;
