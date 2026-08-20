# ADR-0028: The web host mints its Google token at the authority

**Status:** accepted · 2026-08-19
**Context:** #577, the hourly-popup elimination plan. Narrows
[ADR-0005](0005-context-polling-lives-in-the-client-core.md) (device polling
remains the display path; only the web host's *source* of the calendar
token moves), gains a Status entry on
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md)
(the `device` token's 401 re-prompt semantics are why the new route never
answers 401), and partially reverses
[ADR-0011](0011-context-ingestion-moves-server-side.md)'s #486 amendment for
this one consumer (a second, dedicated Google credential, because this one
hands a token to a browser rather than staying server-side).

## The problem

`client/web/src/google/gis.ts` mints the calendar access token with GIS's
token model — `accounts.oauth2.initTokenClient(...).requestAccessToken({
prompt })`. That model is popup-based for **every** prompt value, including
`prompt: "none"`: with a live Google session the popup opens and
auto-closes in well under a second, but it opens. Three triggers reach the
same call — the proactive rotation timer (~55 min), the start-up silent
re-mint, and any core 401 → `CredentialEvent` re-mint — so the installed PWA
spawns a visible popup roughly hourly, on Chrome desktop and Safari macOS
alike. Several module headers in this repo asserted the mechanism was a
hidden iframe round-trip; it never was, and that belief was itself part of
the bug.

## Decision

**The web host stops minting Google credentials in the browser. The
authority mints a `calendar.readonly` access token from a server-held
refresh token — the same mechanism `server/calendar-poll/` already uses —
and serves it to an authenticated device over `POST
/api/google/calendar_token`.**

The device goes on polling Google Calendar directly; only where the token
comes from changes. ADR-0005's contract survives verbatim: the host still
owns the OAuth lifecycle, still pushes tokens into the core at init and on
every rotation, the core still persists no credential and still raises
`CredentialEvent` on a 401, and `hb.calendar.connected` still gates whether
a device calls the route at all. `client/core/` and the wasm seam see no
diff — the host-push shape at the seam is unchanged, only what supplies the
pushed value.

### Route contract

| | |
| --- | --- |
| `POST /api/google/calendar_token` | no body, `device` scope |
| 200 | `{"access_token":"ya29.…","expires_at_ms":…}`, `cache-control: no-store` |
| 401 / 403 | empty — bad device token / wrong scope |
| 503 / 502 | secrets unset / transport, `invalid_grant`, upstream |

`expires_at_ms` is absolute and already carries 60s of slack, so it drops
straight into `TokenResult.expiresAtMs` and `msUntilRotation` with no client
arithmetic. **No path answers 401 for a provisioning or upstream failure**
— under ADR-0004 a 401 means "re-prompt the credential," and this
credential (the `device` token) is fine in that case; a false 401 would make
the client discard a working device token over a Google-side outage.
Secrets unset is 503 (mirrors `ADMIN_SECRET`'s fail-closed posture);
unreachable / `invalid_grant` / other upstream failure is 502. The verdict
is checked before any secret is read, so an unauthenticated caller gets 401
and never learns whether the lane is provisioned at all.

*Amended 2026-08-19 (#577, post-review): "before any secret is read" states
the observable property, not the mechanism. The three `GOOGLE_CALENDAR_*`
bindings are read once in the Durable Object's constructor, beside
`ADMIN_SECRET` and `FCM_SERVICE_ACCOUNT` — reading a binding into the isolate
is not a credential use. What is gated on the verdict is everything a caller
could observe: `worker/src/lib.rs` consults the minter only after `handle()`
returns 204, and a 401 is answered empty, so an unauthenticated caller still
cannot tell a provisioned lane from an unprovisioned one.*

### The credential

ADR-0011's #486 amendment settled on **one broad Google credential**,
shared by the sweeper and both Google poller lanes, carrying Tasks +
`gmail.modify` + `calendar.readonly`. Google's `refresh_token` grant returns
a token bearing the *whole* grant it was minted with — `scope` passed to
that grant type is ignored, so there is no way to down-scope a request at
exchange time. Reusing the shared credential here would mean a stolen
`device` token — the one credential class every web browser already holds —
also mints a bearer that can modify the operator's Gmail, which is a real
widening under CLAUDE.md's blast-radius rule, into a place that credential
class has never reached.

**A second, dedicated `calendar.readonly`-only refresh token is minted
instead** — `GOOGLE_CALENDAR_CLIENT_ID` / `_SECRET` / `_REFRESH_TOKEN`. #486
declined a second credential because both consumers of the shared one were
server-side and the operator cost of a second consent/rotation outweighed a
kind-vs-name narrowing that stayed inside the same trust boundary. That
argument does not carry over here: this credential's consumer is a browser,
which is a different trust boundary than "another GitHub Actions workflow,"
and the blast radius of getting it wrong is Gmail write access reachable
from a stolen device token rather than from a leaked Actions secret.

*Amended 2026-08-20 (#581): **the down-scoping premise above is false, and
the decision it argued for stands anyway.** #581's provisioning ran the
check as its own acceptance criterion, against the shared credential where
the question is actually decidable: asking that three-scope grant for
`calendar.readonly` alone returns a token Gmail and Tasks both refuse with
403 `insufficient authentication scopes` while Calendar answers 200 — a real
narrowing, not an echo in the response body. Google **does** honour `scope`
on a `refresh_token` grant. Two reasons survive and are now the whole case
for the dedicated credential: this lane's secret store never holds a
Gmail-capable token at all, which is strictly smaller than holding one and
narrowing it per request; and it can be revoked without taking down the
sweeper and both pollers. The measurement also earned a change — because
narrowing at exchange time works, `authority/src/google_calendar.rs` now
sends `scope=…calendar.readonly` on every exchange, not as this lane's
defence (the credential is already narrow) but so that pasting the shared
refresh token into `GOOGLE_CALENDAR_REFRESH_TOKEN` fails closed instead of
silently handing every browser a `gmail.modify` bearer.*

### Caching, and why it is a security property

One `RefCell` on the Durable Object instance, not persisted. The DO is the
workspace singleton, so N devices × 1 rotation/hour collapses to one
upstream exchange per hour. Persisting a plaintext Google bearer would be a
new class of stored credential — the `tokens` table holds only sha256
digests — and eviction on a cache miss costs one extra exchange, the same
call `server/worker/src/fcm.rs`'s `FcmSender` already makes and documents
(its `RefCell<Option<AccessToken>>` cache). Not the
Workers Cache API either: that is zone-keyed by URL, which would put a
bearer behind a guessable path.

The cache is not only a performance choice: it caps what a stolen `device`
token can do to Google's token endpoint at **one exchange per hour**,
regardless of how many times the route is called. The module implementing
it must say so in its header.

*Amended 2026-08-19 (#577, post-review): that cap is **steady-state, not a
hard throttle**, and the module header says so in those terms instead. The
cache holds only successful exchanges, so requests that overlap a cache miss
can each start one, and a credential Google is refusing (`invalid_grant`) is
re-attempted on every call rather than remembered. Closing both would take an
in-flight lock and negative caching inside `server/worker`, which has no test
harness; what is actually exposed is one personal workspace's own dead
credential against Google's rate limits, so the claim was narrowed and the
mechanism left alone.*

*Amended 2026-08-19 (#577, post-review): the freshness boundary is **one
re-mint margin ahead of `expires_at_ms`**, not the deadline itself —
`google_calendar.rs`'s `CACHE_REMINT_MARGIN_MS`, deliberately larger than the
web client's 5-minute `msUntilRotation` margin. Without it the two constants
deadlock: the client wakes to rotate while the server still calls the cached
token fresh, gets back the identical token and expiry, and its rotation
effect — keyed on that expiry — never arms another timer, so proactive
rotation dies after its first cache hit and every session rediscovers expiry
through a live 401.*

### Server shape

The precedent is already in the repo and is copied, not invented:
`server/authority/src/google_oauth.rs` (#579) holds the pure,
natively-tested Google OAuth2 token half — the token endpoint, the
expiry-slack policy, the `AccessToken` value, and the response parser —
shared by `fcm.rs`'s JWT-bearer assertion grant and this route's
`refresh_token` grant alike, so the second consumer is a caller instead of
a copy. Each consumer keeps its own runtime shim in `server/worker` —
`fcm.rs` today, and a calendar-token twin here — holding only the `fetch`
call and the `RefCell` cache described above: no string literal and no
status arithmetic of its own. CLAUDE.md's thin-worker rule, sharpened by
the fact that `server/worker` has no test harness at all, so anything
expressed there is untested by construction.

## Rejected alternatives

- **The hidden iframe** (`prompt=none` + `response_mode=web_message`) — the
  mechanism four module headers in this repo already, wrongly, claimed was
  in use. It works on Chrome, because Chrome still allows the third-party
  cookie round-trip to `accounts.google.com` inside a frame. It fails on
  Safari, where Intelligent Tracking Prevention blocks exactly that
  cookie — so shipping it would trade a popup on Chrome for a dead calendar
  on Safari, not eliminate the popup. Rejected outright; not a partial fix
  to build on.
- **Egress above the DO, à la ADR-0018.** ADR-0018 puts the skill-runner
  proxy's fetch above the DO dispatch because that lane needs to stream and
  would otherwise create an await-cycle back into the DO. Neither reason
  applies here: this route returns one small JSON body, not a stream, so
  there is no framing problem the DO's `ApiResponse { status, body: String
  }` shape can't express; and minting a calendar token creates no
  callback into the authority's own API, so there is no cycle to break.
  Moving the egress above the DO would forfeit the single cache point
  (the `RefCell` above) for a shape this route has no reason to pay for.
- **Any attempt to keep minting in the browser** (a timing trick, a
  visibility gate on the rotation timer, a different `prompt` value).
  Named because it will be tempting to retry: while the credential is
  minted in the browser, *every* `requestAccessToken` call opens a popup,
  regardless of prompt value or timing. The only route to zero popups is to
  stop minting in the browser at all.

## Consequences

- **Per-device Google consent and per-device revocation are gone for this
  provider.** Under the old model, each device ran its own OAuth consent
  and could be revoked individually at Google. Under this decision there is
  one grant — the dedicated `calendar.readonly` refresh token — covering
  every web device; revoking calendar access for one device now means
  disconnecting `hb.calendar.connected` on that device rather than revoking
  a distinct Google grant. This is a real loss, stated plainly rather than
  buried in a route table: it is the price of moving the mint server-side.
- **The DO cache is what bounds the cost of that loss.** A stolen `device`
  token — already write-everything against the authority's own API — gains
  one more power, minting a `calendar.readonly` Google token, but the
  Durable Object's one-exchange-per-hour cache means it cannot be used to
  hammer Google's token endpoint or to mint tokens faster than the
  legitimate rotation cadence already does.
- **The client's credential surface shrinks, net.** One OAuth client (the
  web browser client in the Google console) goes away, and so does the
  `https://accounts.google.com` origin from each of the three
  `client/web/csp-worker/csp.ts` directives that carry it today: `script-src`
  (:47, required by GIS loading `https://accounts.google.com/gsi/client` as
  a same-document `<script>` tag), `connect-src` (:49, required by GIS's own
  XHRs to that origin while minting a token), and `frame-src` (:53 — the one
  entry that actually traces to the hidden-iframe belief, since GIS's silent
  re-mint round-trips through a hidden iframe served from that origin).
  **`connect-src`'s `https://www.googleapis.com` allowance stays** — the
  device keeps polling Google Calendar directly under this ADR, from the
  wasm core's own transport, and that allowance is what its calls use. #577
  counts this as "one OAuth client and three CSP allowances" going away
  (#584, #586); read that as the three `accounts.google.com` origins above,
  one per directive, not as three whole directives disappearing.
- **The three new secrets are provisioned like the credential-blast-radius
  rule's most sensitive tier.** `GOOGLE_CALENDAR_CLIENT_ID` /
  `_SECRET` / `_REFRESH_TOKEN` become Wrangler secrets on
  `hummingbird-authority`, set from the operator's terminal, never GitHub
  Actions — beside `ADMIN_SECRET` / `FCM_SERVICE_ACCOUNT` / `RUNNER_*`. Any
  one missing fails the lane closed with a 503, never a silent downgrade.
- **First-time consent moves out of the client entirely.** Consent now
  happens once, in the operator's terminal, when the dedicated credential
  is minted (#581). A browser that has never seen Google gets a working
  token on its first request; nothing stays behind in the client for a
  first-consent flow to do.
