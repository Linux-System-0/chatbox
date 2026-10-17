//! Example ChatBox server plugin (dynamic library, plugin API v2).
//!
//! It rewrites a message before it is persisted, replacing anything that looks
//! like an email address with `[email]`. Build it with
//! `cargo build --release` and copy the resulting shared library next to
//! `pii-redactor.json` under the server's `PLUGIN_DIR`.
//!
//! The ABI structs below mirror `backend/crates/plugin/src/abi.rs`; keep them
//! in sync when the host ABI changes.

use std::ffi::{CStr, CString, c_char, c_void};

/// Must equal the host's `PLUGIN_API_VERSION`.
const PLUGIN_API_VERSION: u32 = 2;

type HookFn = unsafe extern "C-unwind" fn(*const c_char, *const HostApi) -> *mut c_char;
type FreeFn = unsafe extern "C-unwind" fn(*mut c_char);

#[repr(C)]
pub struct HostApi {
    api_version: u32,
    ctx: *mut c_void,
    call: Option<
        unsafe extern "C-unwind" fn(
            ctx: *mut c_void,
            method: *const c_char,
            params: *const c_char,
        ) -> *mut c_char,
    >,
    free_string: Option<unsafe extern "C-unwind" fn(*mut c_char)>,
}

#[repr(C)]
pub struct PluginVTable {
    api_version: u32,
    name: *const c_char,
    version: *const c_char,
    description: *const c_char,
    on_message_before_persist: Option<HookFn>,
    on_message_after_persist: Option<HookFn>,
    on_user_register: Option<HookFn>,
    on_group_created: Option<HookFn>,
    free_string: Option<FreeFn>,
}

static NAME: &[u8] = b"pii-redactor\0";
static VERSION: &[u8] = b"1.0.0\0";
static DESCRIPTION: &[u8] = b"Replace email addresses with [email]\0";

/// Entry point called by the host after loading the library.
///
/// # Safety
/// The host passes valid pointers to a writable vtable and a host table.
#[no_mangle]
pub unsafe extern "C-unwind" fn chatbox_plugin_init(
    api_version: u32,
    vtable: *mut PluginVTable,
    _host: *const HostApi,
) -> i32 {
    if api_version != PLUGIN_API_VERSION || vtable.is_null() {
        return 1;
    }
    // SAFETY: the host guarantees a valid, writable vtable.
    let vtable = unsafe { &mut *vtable };
    vtable.api_version = PLUGIN_API_VERSION;
    vtable.name = NAME.as_ptr().cast();
    vtable.version = VERSION.as_ptr().cast();
    vtable.description = DESCRIPTION.as_ptr().cast();
    vtable.on_message_before_persist = Some(before_persist);
    vtable.free_string = Some(free_string);
    0
}

unsafe extern "C-unwind" fn before_persist(
    context: *const c_char,
    _host: *const HostApi,
) -> *mut c_char {
    if context.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the host passes a NUL-terminated JSON document.
    let Ok(context) = (unsafe { CStr::from_ptr(context) }).to_str() else {
        return std::ptr::null_mut();
    };
    let Ok(context) = serde_json::from_str::<serde_json::Value>(context) else {
        return std::ptr::null_mut();
    };
    let Some(content) = context["payload"]["content"].as_str() else {
        return std::ptr::null_mut();
    };
    let result = serde_json::json!({
        "outcome": "rewrite",
        "content": redact(content),
    });
    CString::new(result.to_string())
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C-unwind" fn free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the pointer came from `CString::into_raw` above.
        drop(unsafe { CString::from_raw(value) });
    }
}

fn redact(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut word = String::new();
    for ch in content.chars() {
        if ch.is_whitespace() || matches!(ch, ',' | '.' | ';' | '!' | '?' | '(' | ')') {
            flush(&mut word, &mut out);
            out.push(ch);
        } else {
            word.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn flush(word: &mut String, out: &mut String) {
    if word.contains('@') && word.contains('.') {
        out.push_str("[email]");
    } else {
        out.push_str(word);
    }
    word.clear();
}
