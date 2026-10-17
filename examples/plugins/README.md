# Example plugins

`examples/plugins/` demonstrates both plugin tiers described in
[`docs/plugin-system.md`](../../docs/plugin-system.md).

## Client plugins

These files demonstrate the runtime client-plugin format. They are installed by
a user, parsed and stored locally for the signed-in account, and never uploaded
to the server.

- `zh-CN.json` — Simplified Chinese language pack.
- `midnight.json` — custom dark theme.

Install one from the Web client's plugin manager or from the Android chat
screen's plugin manager.

Bundled language packs must translate every key in the clients' base
dictionaries. Validate them with:

```bash
node scripts/check-language-packs.mjs
```

## Server plugins

Server plugins are built, deployed, and enabled by the server administrator.

- `server/redactor/` — a dynamic-library plugin that rewrites messages to hide
  email addresses. It demonstrates the plugin API v2 hooks, the JSON result
  contract, and the required `free_string` ownership rule.

See [`server/redactor/README.md`](server/redactor/README.md) for build and
install instructions and for the host API overview.
