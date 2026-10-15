# ADR-0040: Sign-in mints the device token; devices and calendar choices move in-app

**Status:** proposed · 2026-10-11 — accepted when the PR that adds it merges
**Context:** the operator asked to "simplify the settings across devices: a
single login, and all my settings come with me on every device." Until now
a device got its credential one way: the operator minted a `device` token
from a terminal holding `ADMIN_SECRET` (`scripts/mint-device-token.sh`),
stored it in 1Password and pasted it into the web Settings card, the
phone's Settings card or the capture extension's Options page — once per
device, and again after every uninstall or cleared site data. Taking one
away was a terminal `DELETE /api/admin/tokens/:id`. Calendar choices —
whether a device polls Google Calendar at all and which calendars — were
re-picked on every device. This ADR decides what "one login" means here
without giving up the property ADR-0004 and ADR-0008 chose personal tokens
to protect, how a device is seen and removed from inside the app, and
which device-local settings start syncing. Amends
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md),
[ADR-0005](0005-context-polling-lives-in-the-client-core.md),
[ADR-0008](0008-the-authority-is-an-app-owned-server.md),
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md) and
[ADR-0037](0037-the-browser-captures-the-current-tab.md). Terms
**Device credential** and **Source connection** are amended in
`CONTEXT.md`.

## The decision

1. **Signing in with Google mints the device token, and that is all it
   does.** A new unauthenticated route, `POST /api/auth/google`, takes a
   Google ID token plus the device's name and platform (`web`, `android`
   or `chrome` — a closed set), verifies the token, and mints an ordinary
   `device` token for that device: `hb_` plus 64 hex, sha256 at rest,
   individually revocable, returned in plaintext exactly once in the 201,
   exactly as the admin lane returns one. The device stores it where it
   always has (IndexedDB, Keystore-backed storage, `chrome.storage.local`)
   and pushes it into its core through the existing `push_api_key` door.
   Nothing after that moment knows the token came from a sign-in. **There
   is no session, no cookie, no refresh token and no expiry:** identity is
   checked once, at mint time, so a device dark for weeks still wakes up
   and drains its queue, and a 401 still holds the queue and re-prompts —
   now with a Sign-in button in front of the paste field.

2. **Exactly one Google identity can sign in.** The verifier accepts a
   token only when `iss` is Google's, `aud` is one of the configured
   sign-in client ids, `alg` is RS256 against Google's published JWKS,
   `exp` has not passed and `iat` is under ten minutes old,
   `email_verified` is true, and `sub` equals `GOOGLE_SIGNIN_ALLOWED_SUB`
   — the operator's stable Google subject id, a Worker **secret** set from
   the operator's terminal. `sub`, not the email address, because an
   address can be re-pointed and a subject cannot. The client ids are a
   wrangler **var** (`GOOGLE_SIGNIN_CLIENT_IDS`): they are public by
   design. Either unset → 503, failing closed as an unset `ADMIN_SECRET`
   does. A rejected token → an empty 403, never 401 (ADR-0004's
   401-means-re-prompt semantics stay reserved for a held credential the
   authority refuses), and the rejected `sub` is logged so the operator
   can find their own on the first try.

3. **One new Google OAuth client, of type Web application, is the
   audience for all three surfaces, and it is not the calendar client.**
   Revoking or rotating one must never break the other.
   - **Web:** a full-page redirect to Google's authorization endpoint with
     `response_type=id_token`, scope `openid email` and a nonce kept in
     `sessionStorage`, returning to an app route `/signin` that reads the
     token from the fragment and checks the nonce. Not Google Identity
     Services' button: that needs `accounts.google.com` back in
     `script-src`, `connect-src` and `frame-src`, which #577/#586 removed
     from the CSP on purpose. A top-level navigation is governed by none
     of them, so the CSP does not change.
   - **Android:** Credential Manager's Sign in with Google option with the
     web client id as `serverClientId`, which yields an ID token whose
     `aud` is that same id. An Android-type client registered for the
     package and signing SHA-1 is also required by Google; it is never an
     audience.
   - **Extension:** `chrome.identity.launchWebAuthFlow` with the same
     `response_type=id_token` URL, redirecting to
     `https://<extension-id>.chromiumapp.org/` — stable because
     `manifest.json` carries a `key`. `getAuthToken` is not used: it
     yields only access tokens. The manifest gains the `identity`
     permission.

4. **The decidable half of verification is native and tested; the worker
   only fetches and verifies a signature.** Splitting and base64url-decoding
   the JWT, parsing the JWKS and its `max-age`, converting a JWK to SPKI
   DER, checking every claim and the allowlist, validating the name and
   platform, deriving the token id and the mint cap all live in
   `hummingbird-authority`, natively tested. The worker shim fetches and
   caches the JWKS (re-fetching on an unknown `kid`) and calls
   `crypto.subtle.verify`. Nothing new in the wasm32 build takes an HTTP
   client, a tzdb or a parser crate.

5. **The authority names the token; a mint never reuses an id.** The id
   is `device-<platform>-<slug of the name>`, with `-2`, `-3`… appended on
   collision with *any* existing row, revoked rows included, because
   revoked ids are never reused (ADR-0008). Mints through this route are
   capped at ten per rolling hour, counted from the tokens table itself —
   no new state. The admin lane is unchanged and remains the only way to
   mint `sweeper`, `ingest`, `runner` and `openclaw-agent` tokens, and the
   watch's `device-watch` (ADR-0039 is not amended: the watch still
   receives its token by paste from the phone).

6. **A device token can list and remove devices.** `GET /api/devices`
   returns `device`-scope rows only — id, name, platform (derived from the
   id prefix, so rows minted before this ADR read too), created, last
   seen, revoked, whether it is the caller, and whether it may be revoked.
   `DELETE /api/devices/:id` soft-revokes, idempotently; revoking the
   caller's own id is signing out. Two device-scope holders are refused
   with a 403 from a reviewed constant, as ADR-0031 gates on a token id:
   `runner` and `openclaw-agent`, because removing either needs a terminal
   re-mint and a secret rotation elsewhere, and a tap in Settings should
   never start that. `sweeper` and `ingest` tokens are never listed and
   answer 404. No schema change: platform is derived, and `name` already
   holds the label.

7. **Signed-in devices is a Settings section on the web and the phone.**
   Its ordering and wording — this device first, then most recently seen;
   ages through the existing relative-age decision; the sign-out warning
   naming how many changes stay queued, since clearing a credential keeps
   the queue — sink to the core (ADR-0025). Each client keeps pasting a
   token as a fallback behind "Paste a token instead": it serves
   `localhost` without a configured client, and recovery while Google is
   unreachable.

8. **Calendar choices sync.** Two rows of a new typed vocabulary over the
   `settings` table, on ADR-0034/0038's recipe (frozen key spellings; no
   row or an unreadable row means the safe default; subtracted from
   `Core::bindings`; written through the existing setting-write path):
   - `calendar-connected` — a boolean; no row means not connected.
   - `calendar-selection` — `{"v":1,"calendars":[{"id":…,"horizon":"standard"|"long"}]}`,
     the ticked calendars and each one's horizon.

   Two rows so a toggle and a picker edit never race one CAS; one row for
   the selection because one picker edits it as a set. Every edit goes
   through the core, so a web toggle keeps a long horizon only the phone
   can set, and the web moves onto `calendar/selection.rs` — retiring the
   divergence that module's header records. A device seeds the rows from
   its old local values only after its first completed full sweep (before
   that, "no row" may mean "not pulled yet"), and only when no row exists;
   the loser of two simultaneous seeds adopts the winner; the local values
   are then deleted. Whether a device is *connected* syncs too: since
   ADR-0028 no device holds a grant of its own, so connecting is a
   preference rather than a consent, and an extra device polling hits the
   authority's shared token cache.

## What this does not change

- The token's shape, scope, resting places, never-persisted rule and
  401-holds-the-queue rule (ADR-0004, ADR-0008).
- **ADR-0004's revisit trigger has not fired.** It moves to OAuth "the
  day this serves anyone but its author". Google is used here as a check
  that the operator is the operator, once per device, against an
  allowlist of one; the app still serves one person and holds no
  delegated Google grant from the sign-in.
- Theme, board and pane layout and the skills backend stay per device —
  the operator was asked and declined them. The Dropbox root stays
  per machine for ADR-0036's reason.
- The watch, the runner, the OpenClaw agent, the sweeper and the skills'
  `~/.config/hummingbird/api-token` keep their terminal-minted tokens.
- CORS: none is needed. The web app is same-origin, an extension page
  calling a host in `host_permissions` is exempt (ADR-0037 decision 1's
  argument), and Android is native.

## Consequences

- **The operator's Google account becomes a root for minting device
  tokens**, comparable to `ADMIN_SECRET` for that one scope: whoever holds
  it can mint a write-everything credential. It is bounded by the `sub`
  allowlist, the freshness window and the mint cap, and every mint shows
  up in Signed-in devices. Its protection is the Google account's own
  second factor. `CLAUDE.md`'s "Credential blast radius" is amended in the
  same change.
- **A device token can now revoke other operator devices.** That is denial
  of service only: it cannot mint, and the fix is signing in again.
- **Human tasks:** creating the OAuth clients and the consent screen in
  Google Cloud Console (Testing mode suffices — no refresh token is ever
  issued), setting the secret and the var, and a sign-in on real
  hardware for each surface.

## Considered and not taken

- **One shared token on every device.** Rejected: one lost device could not
  be removed alone, and `request.finished`'s `token_id` could no longer
  name which device acted.
- **Passkeys.** No third-party identity, but WebAuthn's CBOR/COSE parsing
  would need its own natively tested crate, and the first-time setup on
  three surfaces is heavier than one Google tap.
- **Approve from a signed-in device (a pairing code).** No outside
  identity at all, but the first device would still need a terminal mint,
  which is the step the operator asked to lose.
- **Sessions or refresh tokens.** They would break ADR-0004's wake-and-drain
  property for no gain: the threat that exists here is a compromised app,
  which a short-lived token does not narrow.
