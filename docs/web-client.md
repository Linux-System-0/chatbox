# Web Client

The web client is a **SvelteKit** application under `web/`. It is a client-side
SPA that talks to the Rust backend over the HTTP control plane (`/api/v1`) and
the WebSocket data plane (`/ws`). This document describes the UI structure and
the design system; protocol details live in [`api.md`](api.md) and
[`realtime-protocol.md`](realtime-protocol.md).

## Layout

```
+--------------------------------------------------+
| header: brand · language · theme · time · user · logout |
+------------------+-------------------------------+
| sidebar          | toolbar (title · status · search)
|  chats/contacts  +-------------------------------+
|  tabs + lists    | messages (grouped bubbles)    |
|                  +-------------------------------+
|                  | composer                      |
+------------------+-------------------------------+
```

- `routes/+layout.svelte` owns the app shell (header, global tokens, resets).
  The signed-in user chip opens the profile dialog, where the custom user ID,
  display name, and avatar can be changed.
- `routes/chat/+page.svelte` is the main workspace. The sidebar has two tabs —
  **Chats** and **Contacts** (which also holds friend requests and the block
  list) — to keep the navigation compact.
- Direct chats are started from the contact list. Friend requests separate
  incoming requests (with accept/reject actions) from outgoing pending ones.
  Contacts expose remark and remark-avatar editing, and a blocked contact moves
  to the block list until unblocked.
- The chats tab can create a group from contacts. Selecting a group opens its
  member panel, where the owner can promote or demote administrators, admins can
  rename the group or add/kick/mute members, and every member can set a private
  **group remark** that only changes their own chat list. The owner can dissolve
  the group; every member can leave.
- The composer attaches files with the paperclip button. The client reserves an
  upload with `POST /files/presign`, uploads bytes to the returned URL, completes
  the upload, and sends a `kind: "file"` WebSocket frame. File bubbles show the
  filename and size and download through the authenticated content endpoint.
- Realtime changes beyond messages (friend requests, contacts, new direct or
  group conversations, group roles/remarks) arrive as `sync` frames; the client
  refreshes the affected lists automatically. It also refreshes on focus,
  reconnect, and a low-frequency fallback timer, so manual reloads are never
  required.
- `routes/login` and `routes/register` render a centered, branded auth card.

On narrow viewports the sidebar and the conversation occupy the same grid cell;
only one is shown at a time, and the toolbar exposes a back button to return to
the list.

## Design tokens

All visual constants are CSS custom properties declared on `:root` and
overridden under `[data-theme='dark']` in `+layout.svelte`:

| Token | Purpose |
|---|---|
| `--bg`, `--surface`, `--surface-2`, `--surface-3` | Layered backgrounds |
| `--text`, `--muted`, `--border` | Foreground and hairlines |
| `--accent`, `--accent-hover`, `--accent-soft`, `--accent-contrast` | Brand color |
| `--bubble-in`, `--bubble-out`, `--bubble-out-text` | Message bubbles |
| `--danger-*`, `--success` | Semantic states |
| `--shadow-sm`, `--shadow-md`, `--radius*`, `--header-h` | Elevation, shape, layout |

Components consume the tokens only; they never hard-code colors. This is what
makes the light/dark themes and any future theme plugin a drop-in.

## Themes and languages

Both are **client plugins** (see [`plugin-system.md`](plugin-system.md)) and
are installed at runtime from a JSON file. They are never compiled into the
Web bundle.

- `lib/plugins.ts` parses and validates plugin files, stores them per user in
  `localStorage`, and exposes the installed-plugin store.
- `lib/i18n.ts` keeps only the built-in English fallback dictionary. Available
  languages are English plus every installed language pack; components read
  strings through the `translator` derived store.
- `lib/theme.ts` keeps the built-in light/dark schemes and maps an installed
  theme package's colors onto the CSS custom properties. `app.html` applies a
  cached base scheme before first paint to avoid a theme flash.
- The header's plugin manager opens `lib/PluginManager.svelte`, where the user
  selects a `.json` file to install and can remove installed plugins.

Installed plugins are keyed by the signed-in user id, so one account's plugins
are invisible to another account on the same browser. Removing the active
language pack or theme falls back to English and the built-in light scheme.

### Theme package colors

Theme packages provide a `payload.colors` map. The Web client maps these keys
onto its design tokens:

| Package key | CSS token(s) |
|---|---|
| `background` | `--bg` |
| `surface` | `--surface` |
| `surfaceAlt` | `--surface-2`, `--surface-3` |
| `text` | `--text` |
| `muted` | `--muted` |
| `border` | `--border` |
| `primary` | `--accent` |
| `primaryHover` | `--accent-hover` |
| `onPrimary` | `--accent-contrast` |
| `primarySoft` | `--accent-soft` |
| `bubbleIn` | `--bubble-in` |
| `bubbleOut` | `--bubble-out` |
| `bubbleOutText` | `--bubble-out-text` |

## Messages

`chat/+page.svelte` groups consecutive messages from the same sender within
five minutes for avatar/layout purposes, but **every message bubble shows its
absolute send time**. Day separators are inserted whenever the calendar day
changes in the configured time zone; system notices render as centered pills.
Names follow **contact remark → display name → custom user ID**, and avatars
follow **contact remark avatar → profile avatar → generated default avatar**.
The generated fallback derives a stable hue from the numeric user id, so no
profile fetch is required. Group messages show the sender name and avatar in
front of each message group.

### Time display and read receipts

- The header's clock button opens the **time display** dialog. The user chooses
  either the system time zone or a fixed offset from UTC (15-minute steps) and
  either the system's localized date/time format or a custom pattern
  (`YYYY`, `MM`, `DD`, `HH`, `mm`, `ss`, `Z`, with `[literal]` escapes).
  Settings are stored locally per account; the defaults are the system time zone
  and system format.
- Every outgoing message carries a read receipt. A gray `✓` means "not read
  yet"; it becomes a green `✓✓` once every other participant's read cursor
  reaches the message sequence. In a group the accessible label reports the
  read count (`Read by 2/3`), and the check turns green only when all other
  members have read the message.
- `ConversationView.read_cursors` seeds the cursor map; live `read` frames update
  it, so receipts change without polling or reloading history.

Direct-chat presence is taken from the `presence` frames and the presence
bootstrap endpoint, not from the local WebSocket connection status. The local
connection state is shown only while connecting or offline; once connected, the
toolbar reflects whether the other participant is actually online.

## Accessibility

- Interactive controls are real `<button>`/`<input>` elements with labels or
  `aria-label`s; icon-only buttons always carry an accessible name.
- Visible focus rings come from the global `:focus-visible` rule.
- Status is conveyed by text as well as color (the connection pill shows a dot
  and the translated status word).
- Decorative art is `aria-hidden` and does not enter the accessibility tree.
- Destructive actions are confirmed with the in-page `lib/ConfirmDialog.svelte`,
  which traps focus, closes on `Escape`, and sizes itself from the design tokens.
  The web client never uses the browser-native `window.alert`, `window.confirm`,
  or `window.prompt` dialogs, because they cannot be themed, translated, or
  styled and they block the page's own accessibility tree.
