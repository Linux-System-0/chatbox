# Example server plugin: PII redactor

A dynamic server plugin that rewrites `message.before_persist` payloads,
replacing anything that looks like an email address with `[email]`. It does not
call the host API; see [`docs/plugin-system.md`](../../../../docs/plugin-system.md)
for the full ABI and the `HostApi` methods.

## Build and install

```bash
cd examples/plugins/server/redactor
cargo build --release

mkdir -p ../../../../plugins
cp pii-redactor.json ../../../../plugins/
cp target/release/libchatbox_plugin_redactor.so ../../../../plugins/
```

On macOS the library is `target/release/libchatbox_plugin_redactor.dylib`; update
`entry` in the manifest to match. On Windows use the `.dll` name.

Start the server, then enable the plugin as an administrator:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/plugins/server/pii-redactor/enable \
  -H "Authorization: Bearer <access-token>"
```

The plugin stays disabled until enabled, and the enabled state survives
restarts. Disable it with the matching `.../disable` request.

## What to copy from this example

- `chatbox_plugin_init` fills the vtable and checks `api_version`.
- The hook returns a NUL-terminated JSON document and frees it through
  `free_string`.
- Returning `NULL` means "no opinion" and lets the message pass unchanged.
- To call back into the server, use `HostApi::call` with a method from
  [`docs/plugin-system.md`](../../../../docs/plugin-system.md) and release the
  response with `HostApi::free_string`.
