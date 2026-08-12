# ADR-0018: The authority proxies the skill runner

**Status:** accepted · 2026-08-12
**Context:** #273, the microtask button. Extends
[ADR-0008](0008-the-authority-is-an-app-owned-server.md)'s same-origin rule
to a lane it did not anticipate — one where the authority is a *client* of
another service — and upholds
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md)'s
401-means-re-prompt semantics, which is what most of the status table below
is defending. Numbered 0018 because 0017 is reserved by open issue #310.

#272 landed the cloud runner on Fly: `POST /run {skill, args}` streams NDJSON
progress and ends in a terminal envelope, writing `microtask`'s checklist
into the authority's owned `steps` table. #273 is the client half — a button
on an item. Nothing in the browser could reach the runner: `runner/src/server.js`
sends no `Access-Control-*` header and has no `OPTIONS` branch, and the
shell's CSP is `connect-src 'self'` plus two Google hosts.

## The decision

> `POST /api/skills/run` on the authority proxies to the runner's
> `POST /run`, authorized by the caller's existing **device token**, holding
> the runner's base URL and bearer as Worker secrets. The browser talks to
> one origin and holds one credential, exactly as before.

The runner's NDJSON stream is forwarded to the browser unchanged. No CORS
lane, no CSP change, no new client credential.

## Rejected alternatives

- **CORS on the Fly app.** The cheapest change by line count: an
  `Access-Control-Allow-Origin` header and an `OPTIONS` branch in
  `runner/src/server.js`. Rejected because it contradicts ADR-0008's
  `connect-src 'self'` outright — the same reason that ADR rejected a second
  `api.twinion.net` hostname — and it would make the runner's bearer a
  browser-reachable credential, which is the next bullet.
- **A runner bearer shipped to devices.** Then the browser calls the runner
  directly and the authority is uninvolved. #269 rejected key-on-device for
  three reasons that all apply here unchanged: the credential is shared
  rather than per-device, so revoking one device revokes all; it cannot be
  scoped down (the runner has one bearer for every skill); and the runner's
  own `HB_API_TOKEN` is a write-everything device token, so the bearer is
  transitively a write credential on the whole workspace.
- **A job handle plus polling** (`POST` returns an id, the client polls
  `GET /api/skills/runs/<id>`). Strictly more machinery — a run registry,
  a retention rule, a poll cadence that would be a *second clock* against
  ADR-0007's single interval — and #269's "no queue, ever" doctrine reads a
  server-held run whose result outlives the asking view as exactly the queue
  it bans. **Kept as the flip condition:** if edge streaming proves
  unreliable at length, this is the fallback. Never CORS.

## The split: verdict in the pure crate, egress in the shim

The Durable Object cannot express a stream at all — its `handle()` returns
`ApiResponse { status, body: String }`. So the egress happens in the
top-level `#[event(fetch)]`, **above** the DO dispatch.

The decisive reason is not concurrency (a DO interleaves at `await` points
and would not block) but a **cycle**: `microtask.apply` calls back into
`hb.twinion.net/api/steps` with the runner's own `HB_API_TOKEN`, so routing
the run through the DO would make it await a subrequest that needs the same
object to answer. Intercepting above the DO removes the cycle entirely.
Secondary: every DO `fetch` runs `init_schema` + `ensure_alarm_scheduled`
first, and an open request accrues duration and blocks hibernation for the
length of a model call.

But authorization needs the `tokens` table, which is inside the DO. So the
shim sends a **bodiless preflight** — a fresh minimal `Request` carrying the
same URL, `POST`, and `authorization` only — and the pure crate answers the
verdict as a 204. Never `req.clone()`: that would buffer the body inside the
DO and consume the stream the shim still has to forward.

**Order is load-bearing: verdict first, secrets second.** Checking the
secrets first would tell an unauthenticated caller whether the lane is
provisioned.

**No new `auth::permitted` arm.** Its final arm is already
`_ => matches!(scope, Scope::Device)`, which is how every other device-only
route is gated; an explicit arm would be dead code duplicating the default.
The fixtures test the default instead.

## The status table

Every proxy-generated failure is **one NDJSON envelope line** with
`content-type: application/x-ndjson`, so the client has one parser and never
branches on prose. The two exceptions are the statuses that already carry a
client contract.

| Case | Status | Body |
| --- | --- | --- |
| Bad/missing device token | 401 | empty (the DO's verdict, forwarded) |
| Token out of scope | 403 | empty (the DO's verdict, forwarded) |
| Wrong method | 405 | the pure crate's `ApiError` JSON |
| `RUNNER_BASE_URL`/`RUNNER_BEARER_TOKEN` unset | 503 | `"The cloud runner is not configured on this server."` |
| The subrequest errors | 502 | `"Cloud runner unreachable."` |
| The runner answers 401 | **502** | `"The cloud runner rejected this server's credential."` |
| The runner answers 400 or 413 | forwarded verbatim | already the runner's own valid NDJSON |
| The runner answers 200 | forwarded verbatim, streaming | the runner's stream |
| Anything else | 502 | `"The cloud runner answered <status>."` |

Three traps, named because each is what a naive implementation gets wrong:

1. **Unset secrets are a 503, not a 401.** A 401 would be a lie that makes
   the client re-prompt a device token that is perfectly fine (ADR-0004).
   Mirrors `ADMIN_SECRET`'s fail-closed posture. The log names the missing
   variable; the body never does.
2. **Never forward the runner's 401.** This is what "just return the
   runner's response" gets wrong: a `RUNNER_BEARER_TOKEN` rotated on one
   side only would surface to the browser as a 401 against the *user's* own
   credential.
3. **Only 200/400/413 forward verbatim**, because only those three are the
   runner's own JSON. A bare Fly 502 has an HTML body and would break the
   always-NDJSON promise.

Shim-synthesized lines carry **no** `backend`/`model` stamp — nothing was
attempted.

Because `server/worker` has no test harness, every string and every status
above lives in `server/authority/src/skills.rs`, natively fixture-tested;
the shim holds no literal and no status arithmetic of its own.

## Platform facts worth recording

- **CPU time is the Worker cap, not wall clock**, and awaiting a `fetch`
  burns none — a run that takes a minute of model time is not a Worker
  problem.
- **Two subrequests per tap** (the DO preflight and the runner call), far
  inside any limit.
- **The runner's 20s heartbeat now defends the Cloudflare hop too**, not
  just Fly's 60s idle kill. Anyone "optimizing it away" breaks this proxy.

## What this obliges

- `RUNNER_BASE_URL` and `RUNNER_BEARER_TOKEN` are Cloudflare Worker secrets,
  set from the operator's terminal, **never GitHub Actions** — the bearer is
  transitively a write credential plus a spend faucet (CLAUDE.md,
  "Credential blast radius").
- **Rotating `RUNNER_BEARER_TOKEN` is now a two-place operation**:
  `fly secrets set` on the runner *and* `wrangler secret put` here. Every tap
  answers 502 in between.
- A new proxy-visible failure mode gets its prose in
  `server/authority/src/skills.rs`, not in the shim.
