# hummingbird capture — the browser extension (ADR-0037)

One click turns the current tab into a new item: the tab's title as the
item's title, its URL as the item's **Link** (shown by its host, as an
Android share lands), an optional description, and the web capture box's
own three destinations — the inbox square (Triage, the default, also
Enter), the mint square (born in Ready, for a thing already startable),
and the mint-for-today square (Ready with today's date as the deadline).
The popup is the whole extension: no background worker, no content script,
no server change.

```
click (or Alt+Shift+H)  ->  popup: title, description, three squares  ->  POST /api/items
```

It POSTs straight to `https://hb.twinion.net` from the popup page. The
authority has no CORS lane by decision (ADR-0008/0018) and needs none: a
fetch from an *extension page* to a host in `host_permissions` is exempt
from CORS in Chrome and Firefox alike. A content script or a bookmarklet
runs in the page's origin and would be refused, which is why neither is
the shape.

## What it duplicates from the app, and why

`lib.js` is a third copy of two rules `client/core/src/decisions/share.rs`
owns: **a title is never a raw URL**, and an empty title falls back to the
URL's host, whose derivation (`urlHost`) is a line-for-line port of
`url_host`. The core's rule reaches the web through a wasm seam; pulling
that build in here would make this a `client/**` package, and the input is
a tab's `{title, url}`, not a text share. So the copy is pinned by test
(`lib.test.js` carries `share.rs`'s own cases) rather than sunk at runtime —
the same shape ADR-0025's #500 amendment chose for `field-vocabulary.ts`.
If `url_host` changes, `urlHost` changes.

The wire body is exactly `{id, title, link_url}` plus `description` when
one was typed, `stage: "ready"` for a mint, and `deadline` for the today
square. Triage sends no `stage` (the route defaults it); `link_label` is
never sent (the host stands in); no `source*` column is written — a click
is a choice, not a capture source the system drains.

## Install

Once per browser profile, per machine.

1. **Mint a device token, vault-first.** The extension is an operator
   device like a phone or a laptop shell, with its own token id so it can be
   revoked alone and told apart in `request.finished` logs:

   ```sh
   ./scripts/mint-device-token.sh device-chrome-hal2024 \
     "HAL2024 Chrome capture extension" hummingbird-device-chrome-hal2024
   ```

   Read that script's header first. A lost plaintext burns the id
   permanently; the two read-only prechecks in `docs/openclaw.md`'s runbook
   (id absent from `GET /api/admin/tokens`, `op item get <title>` misses)
   are worth the minute.

2. **Load unpacked.** `chrome://extensions` → Developer mode → *Load
   unpacked* → this directory. **Load it from a stable path**: an unpacked
   extension's id is derived from its directory, and the token lives in
   `storage.local` *per id*, so loading from a different checkout or
   worktree is a different extension with no token. Chrome expects the
   "Unrecognized manifest key 'browser_specific_settings'" warning under
   *Errors*; the key is Firefox's, mandatory for signing there, and Chrome
   ignores it at runtime.

3. **Paste the token.** Extension → *Options* → paste from 1Password
   (`op read op://dev/hummingbird-device-chrome-hal2024/password`) → Store.
   The page trims a trailing newline and refuses anything that is not
   `hb_` + 64 hex; nothing else is checked until the first save.

4. Pin the toolbar button. `Alt+Shift+H` opens it from the keyboard; Enter
   in the title or Ctrl+Enter in the description lands in Triage. A mint
   is a click on its square, never a keystroke.

### Rotation

`DELETE /api/admin/tokens/device-chrome-<machine>` on the authority, a fresh
mint under a **new** id (revoke is a soft delete that does not free the
id), Options → Forget → paste. `CLAUDE.md`'s "Credential blast radius"
section is the standing text on what a device token can reach.

## What the popup says, and what it means

| Message | Meaning |
| --- | --- |
| Added to Triage / Minted into Ready / Already saved | 201 at that destination, or a 200 replay of an id this popup already sent. The popup closes. |
| Only http(s) pages can be captured. | `chrome://`, `about:`, `file:` — nothing was sent. |
| The token was rejected. | 401. Options → paste a fresh one. |
| This token cannot create items. | 403 from the authority: not a `device` (or `sweeper`) token. |
| Blocked at the edge (Cloudflare) | 403 as an HTML page — the edge, not the authority. |
| The authority answered with a page, not the API. | A 200 that was the static shell, or a row with the wrong id. Nothing was saved. |
| Could not reach hb.twinion.net. | Network. Save again — the same id is replayed. |

**Saves survive the popup closing.** The id is written to `storage.local`
before the fetch; reopening on the same page within ten minutes reuses it,
and the route's idempotency by client id answers 200 instead of minting a
second item.

## Testing

```sh
cd tools/browser-capture
node --test            # lib, manifest and token-drift suites; CI runs the same
node copy-tokens.js    # after a change to client/web/src/design/tokens/
npx web-ext lint       # optional, local only: needs the network
```

The manual pass, against the live authority with a real token, is the
verification list in ADR-0037. There is no fixture server: the only thing
the pure suite cannot see is the browser itself.

## Firefox and Safari

The manifest is already Firefox-shaped (Manifest V3, an `action` popup, no
background script, a `gecko.id`); `about:debugging` → *Load Temporary
Add-on* runs it unchanged, host permission granted at install from Firefox
127. A temporary add-on is wiped on restart, token included — a signed,
self-distributed `.xpi` is the durable install. Safari is
`xcrun safari-web-extension-converter` over this directory on a Mac, a
later slice.
