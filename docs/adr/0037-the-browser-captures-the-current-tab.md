# ADR-0037: The browser captures the current tab — a popup that writes to the authority as a device

**Status:** accepted · 2026-09-08
**Context:** the operator wanted one click in the desktop browser to turn
the page in front of them into a new item — Chrome first, Firefox and
Safari later. #782 had already decided what a shared page *is* to an item
(a Link, set at capture, shown by its host) and where that decision lives
(`client/core/src/decisions/share.rs`, ADR-0025), and its header
anticipated "a web share target whenever one is wanted". Nothing in the
repo reached the authority from a browser other than the PWA's own sync
engine, and the authority serves no CORS (ADR-0008, reaffirmed in
ADR-0018). This ADR settles how a third browser surface gets a capture in.
It adds a member kind to the `device`-token population named in
`CLAUDE.md`'s "Credential blast radius" (the sentence there is amended in
the same change); it names, and does not resolve, a tension with
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md).

## The decision

1. **A popup-only Manifest V3 extension, and the popup page does the
   write.** `tools/browser-capture/` — a manifest, a popup, an options
   page, and one pure module. No background worker, no content script. The
   popup calls `tabs.query` under `activeTab`, seeds a form, and `POST`s
   `/api/items` itself. This is the only zero-server-change shape: a fetch
   from an *extension page* to a host in `host_permissions` is exempt from
   CORS in Chrome and Firefox, while a content script or a bookmarklet
   runs in the page's origin and is refused. The authority stays
   same-origin with the PWA, sends no `Access-Control-*` header, and
   handles no `OPTIONS`; `connect-src 'self'` is the PWA's document CSP and
   never bound the extension. Dropping the background worker also removes
   the largest Chrome/Firefox MV3 divergence (`service_worker` versus
   `scripts`), which is what keeps the later ports a manifest away.

2. **The extension is an operator device.** It holds its own `device`
   token, id `device-chrome-<machine>`, minted vault-first with
   `scripts/mint-device-token.sh` into 1Password as
   `hummingbird-device-chrome-<machine>`, pasted once into the options page
   and kept in `storage.local` (never `sync`). It joins the blast-radius
   population as a full write credential with a spend faucet, revocable
   alone and named in the authority's logs by its id (#711) — exactly
   ADR-0029 decision 6's move for the OpenClaw agent, for the same reason:
   nothing narrower than `device` exists, and inventing a scope is
   authority work out of proportion to a popup. A browser profile is a
   weaker vault than a mode-600 file, and the PWA's own token already rests
   in that same profile's IndexedDB; the extension adds a second copy of
   the same standing, not a new class of exposure.

3. **The tab is a Link, not a source, and it lands where the web's
   capture box lands.** The body is `{id, title, link_url}` plus
   `description` when typed — the shape an Android share lands — under the
   web capture box's own three destinations (`capture-destination.ts`):
   Triage, the default and the Enter key, sends no `stage` (the route
   defaults it, and stating it would copy a fact the server owns, as
   sweep.py says of itself); a mint sends `stage: "ready"`; mint-for-today
   adds today's local date as `deadline`, `todayDeadline` re-stated.
   `link_label` is never sent (the host stands in, `link_display_label`);
   no `source`, `source_key` or `source_url` is written.
   The glossary keeps those apart: a source URL is *where an item came
   from*, written by a capture source the system drains and acks
   (ADR-0002/0019); a Link is *a choice*. A click on a page is the operator
   choosing what an item is about, not an inbox being emptied.

4. **The share mapping's title rules are re-stated in JS, pinned by
   test.** `lib.js`'s `urlHost` is a line-for-line port of `share.rs`'s
   `url_host`, and `draftFromTab` applies its two rules — a title is never
   a raw URL; an empty one falls back to the host — with `lib.test.js`
   carrying `share.rs`'s own cases. This is the ADR-0025 tension, named:
   decisions sink to the core, and this client cannot reach the core
   without pulling `client/ffi-web`'s wasm build into a directory that must
   stay a flat, dependency-free tree the browser loads by path — and the
   input is a tab's `{title, url}`, not a `text/plain` share. The answer is
   the one ADR-0025's #500 amendment already chose for values a client
   cannot take at runtime: a pinned copy, gated by test, not a runtime
   sink. It is a third copy (core, and the two seams' tests) and the
   header says so; if `url_host` changes, `urlHost` changes.

5. **A save survives the popup dying.** A popup is destroyed the moment it
   loses focus, so a fetch in flight may land after its page is gone. The
   popup mints `crypto.randomUUID()` on open and writes `{id, url,
   mintedAt}` to `storage.local` *before* fetching; a reopen on the same
   URL within ten minutes reuses that id, and `POST /api/items`'s
   idempotency by client id (ADR-0008) turns a doubled save into a 200.
   That 200 carries the stored row, not the new payload, so the popup
   compares: the same capture is reported as already saved; a different
   one means the earlier save landed and this is a new item, re-posted
   once under a fresh id. Only a classified save clears the pending id —
   a misrouted 200 keeps it.

6. **Responses are classified content-type first, status second.**
   sweep.py's model, for sweep.py's reasons: the authority shares an
   origin with the PWA, so an unmatched path is a 200 from the static
   shell, and Cloudflare's Browser Integrity Check answers a 403 in HTML.
   A 2xx is "saved" only when it is JSON carrying the id that was sent;
   401 is the token; 403 is scope unless the body is a page; 400 shows the
   authority's own message; anything else keeps the form so Save retries
   the same id.

7. **It lives under `tools/`, with its own workflow and a token drift
   gate.** Not `client/`, whose `client/**` filter would run the Rust/wasm
   job for a popup edit; `browser-capture.yml` runs `node --test` and
   parses the page scripts. The design tokens are *copied* into
   `design/tokens.css` by `copy-tokens.js` (an unpacked extension is a
   flat directory; a link out of it does not load) with `fonts.css`
   reduced to its `:root` stacks — its `@font-face` rules point at
   `/fonts/…`, which inside an extension origin resolves nowhere — and
   `tokens.test.js` fails when the copy and the web disagree, the
   ADR-0026 shape as a test. The workflow watches the web tokens' path for
   that reason.

## Consequences

- `CLAUDE.md`'s device-token population gains a member kind. Provisioning
  is the runbook `docs/openclaw.md` already carries, with a different id
  and no file placement; the README in `tools/browser-capture/` is the
  install and the manual test.
- Loading unpacked ties the extension's identity, and so its stored
  token, to the directory it was loaded from. Load from a stable checkout;
  a different worktree is a different extension.
- Chrome logs "Unrecognized manifest key 'browser_specific_settings'" on
  every reload. Accepted: the key is mandatory for Firefox signing, Chrome
  ignores it at runtime, and a second manifest for one operator is not
  worth its drift.
- A third copy of `url_host` exists. The pin is a test, not a build step;
  a core change that lands without re-porting fails only when someone
  updates the test cases — the header names the coupling, and that is the
  guard.
- Firefox needs only a signed `.xpi` for the token to survive a restart;
  Safari needs the converter on a Mac. Neither is in this slice.

## Alternatives rejected

- **A content script or a bookmarklet.** CORS-bound in the page's origin;
  would need the CORS lane ADR-0008 and ADR-0018 exist to avoid.
- **A background service worker doing the fetch.** Nothing it would add
  that persisting the pending id does not, and it is the Chrome/Firefox
  MV3 divergence in person.
- **Routing through the PWA** (open `hb.twinion.net` with the page in a
  query, let the sync engine write). No new credential, and the core's
  own mapping — but no popup, a web-app change, a tab that opens on every
  capture, and a URL parameter that submits on load is a replay hazard the
  extension would then have to solve anyway.
- **CORS on the authority, or a second hostname.** Rejected upstream
  (`server/worker/wrangler.toml`, ADR-0018) and not reopened for this.
- **Reaching `share.rs` through the wasm seam.** Decision 4.
- **A narrower scope for the extension's token.** ADR-0029's reasoning,
  unchanged.
- **Writing `source_url` for provenance.** Decision 3.
