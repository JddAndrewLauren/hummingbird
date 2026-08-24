# Diagnosing a sync incident

> **Status (2026-08-23, #712): 6 of the 8 interpretation-table rows below
> are proven; 2 are not.** Rows 1, 2, 3 and 5 are proven with a real HTTP
> round trip against a local `wrangler dev` running the real
> `hummingbird-authority` Worker — not against the deployed
> `hb.twinion.net`. Rows 4 and 6 are proven by two existing,
> already-passing production tests in `client/ffi-web/src/task_host.rs`
> that are exactly the scenario each row names. Rows 7 and 8 are **not
> induced** — see their own rows in the table and their own entries in
> "Proof" below for why. **The table's Proof column names which of these
> three states each row is in; do not read the table as eight uniformly
> proven rows.**
>
> Local, not production, for rows 1/2/3/5: this slice's own commits
> (#706–#711) are on the `diagnostics-slices` integration branch, not yet
> merged to `main`, so the Worker actually serving `hb.twinion.net` today
> predates every event this runbook talks about — a live `wrangler tail`
> against it returns `"logs": []` for every request, not because nothing
> was logged but because the deployed build cannot log it yet. Every
> gesture below is the exact gesture an operator runs against production
> once this lane ships.

This is the runbook for "why didn't my capture show up" / "why is Now
stale" at 11pm on cellular. Gestures first, then how to read what you get,
then (only where it earns its place) the mechanism.

## Export a client's journal

**PWA (web).** Settings → *Diagnostics* card → **Download diagnostics**.
Downloads `hummingbird-diagnostics-<timestamp>.json`
(`client/web/src/shell/diagnostics-download.ts`) — a snake_case envelope,
`{"schema_version", "dropped_count", "events"}`, one flat array of
`DiagnosticEventV1` rows, newest last. **Clear diagnostics** empties the
journal in place; neither button touches the mirror (the offline item
cache) or the sync engine.

**Android.** Settings → *Diagnostics* card → **Export diagnostics**. A
system document picker chooses where to write
`hummingbird-diagnostics-<epoch-ms>.json`
(`client/android/.../SettingsScreen.kt`); the file is the same
`{"schema_version", "dropped_count", "events"}` shape (`DiagnosticJournal.kt`).
**Clear diagnostics** is the same in-place empty as the web side.

Both exports are a manual, on-demand read of an on-device file — see
**Scope** below for what that deliberately does not include.

## Read a `DiagnosticEventV1` row

Every row, on every host, is the same envelope. A real one — `source:"core"`
is the only source that ever names a `cycle_id`/`request_id`
(`client/web/src/worker/diagnostics-events.ts`'s `envelope()` hardcodes all
three ids to `null` on every `web-worker` row, and `http.*` events are
`Source::Core`-only, always carrying a `request_id` — this join's own step
1 below depends on that):

```json
{"schema_version":1,"seq":4,"wall_clock_ms":1787549185637,"elapsed_ms":15,
 "session_id":"live-proof-row5","source":"core","cycle_id":"diag712-row5",
 "operation_id":null,"request_id":"diag712-row5-0",
 "event":{"name":"http.started","payload":{"method":"GET","route":"/api/sweep"}}}
```

`server/domain/src/diagnostics.rs` is canonical for every field and every
`event.name` this can be — its module header, `DiagnosticEventV1`'s own doc,
and the closed `DiagnosticEvent` enum. Three fields this runbook leans on
repeatedly, because reading them wrong flips a diagnosis:

- **`seq` is monotonic per `session_id`, never per `cycle_id`.** A web
  session, an Android process and the authority's one Durable Object
  instance are three independent `seq` counters. Order rows by
  `(session_id, seq)`, never by `seq` alone across a mixed export, and never
  by `cycle_id` — a cycle can span rows from a session that also logged
  unrelated cycles in between.
- **`owner` on `core.busy`, `core.wait_started` and `core.acquired` is
  `Option<CoreOwner>`, and `null` means "this writer could not name one" —
  never "nobody held it."** The TypeScript SharedWorker layer that writes
  most of these is structurally blind to the holder's identity (see
  `diagnostics.rs`'s "cross-language payload rule for the `core.*` quad");
  `core.released`'s `owner` is the one member of the family that is
  required, because only a Rust guard ever produces that event. Reading a
  `null` owner as "the core was free" is the exact misdiagnosis this
  paragraph exists to prevent.
- **`operation.finished{success}` means "committed locally and durably
  queued for send," never "reached the authority."** See **Known gaps**
  below — `operation_id` does not cross the outbound-queue boundary, so this
  event and the eventual `http.*` span for the same logical write share no
  id.

## Query the authority's side (Cloudflare)

The authority (`hummingbird-authority`, Cloudflare Workers + a Durable
Object) logs one `request.received` line for **every** request the Durable
Object handles — `server/worker/src/lib.rs`'s `fetch` logs it before
authentication runs or any other awaited work (`server/worker/src/lib.rs:189`),
deliberately, so it survives a hang anywhere below it — and one
`request.finished` line for every outcome, including a 401, a 403, and a
500, not only a success. Both carry the client-minted
`X-Hummingbird-Cycle-Id`/`X-Hummingbird-Request-Id` headers as `cycle_id`/
`request_id` (`server/authority/src/diagnostics.rs`). `server/worker/wrangler.toml`
sets `head_sampling_rate = 1` (100%) precisely so a missing line means "never
received," not "sampled out."

**Live tail, while reproducing:**

```sh
npx wrangler tail hummingbird-authority --format pretty --search "<cycle-id-or-request-id>"
```

`--search` matches the raw log line, so either id works, and `pretty`
prints each `console_log!` line human-readably as it arrives — this only
sees requests from the moment it connects forward, so start it *before*
inducing the condition.

**After the fact, within the retention window:** Cloudflare dashboard →
Workers & Pages → `hummingbird-authority` → **Logs**, filter by the same id
substring, over the time range the incident fell in. This is the only way
to see a `request.received`/`request.finished` pair once the live tail
window is gone.

## Join one cycle end to end

1. Export the client's journal (above). Filter its rows to one
   `cycle_id`, keep the ones named `http.started`/`http.finished` (the
   ones with a `request_id`).
2. For each `http.started`, take its `request_id` and query the authority
   (above) for a `request.received` naming the same id.
3. **Every `http.started` in the client's journal is accounted for**: either
   it has a matching `request.received`/`request.finished` pair (the call
   reached and was answered by the authority), or it has neither (the call
   never left the device, or never reached the Durable Object) — record
   which, for every row. A `http.started` with a `request.received` but no
   `request.finished` is a live stall, not a missing pair.

This join is what the interpretation table below is read through — a table
row is a *pattern* in the joined evidence, not a single event name.

### Worked exhibit: one cycle, traced end to end

Cycle id `diag712-row5`, the real capture described in row 5's proof
below, traced with the exact three steps above.

**1. Client journal, filtered to `cycle_id: "diag712-row5"`, kept to
`http.*` rows (each carries a `request_id`):**

```json
{"event":{"name":"http.started","payload":{"method":"GET","route":"/api/sweep"}},"request_id":"diag712-row5-0"}
{"event":{"name":"http.finished","payload":{"method":"GET","route":"/api/sweep","status":401,"failure":{"kind":"http","status":401}}},"request_id":"diag712-row5-0"}
```

One `http.started` in this cycle: `request_id: "diag712-row5-0"`.

**2. Query the authority for that `request_id`:**

```json
{"event":{"name":"request.received","payload":{"method":"GET","route":"/api/sweep"}},"request_id":"diag712-row5-0"}
{"event":{"name":"request.finished","payload":{"method":"GET","route":"/api/sweep","status":401,"duration_ms":1,"response_bytes":0,"token_id":null,"auth_result":"rejected"}},"request_id":"diag712-row5-0"}
```

**3. Inventory.** This cycle had exactly one `http.started`; it is fully
accounted for — a matching `request.received`/`request.finished` pair on
the authority side, same `request_id`, same route, same status (401) both
sides. Zero calls in this cycle have no counterpart (contrast row 1's
proof below, where the inventory is one call and its counterpart is
"none" — the other half of this same criterion).

## The interpretation table

| # | Evidence | Interpretation | Proof |
|---|---|---|---|
| 1 | Client `http.started`, no server `request.received` | Failure before the authority | Induced — real round trip |
| 2 | Server `request.received`, no server `request.finished` | Authority/DO stall | Induced — real round trip |
| 3 | Server `request.finished`, no client `http.finished` | Response path or client transport | Induced — real round trip |
| 4 | Sync owns core while UI operation waits (`core.busy`/`core.wait_started` naming the sync session, an interactive `operation.requested` stuck behind it) | Core monopolization confirmed | Induced — existing production test |
| 5 | `request.finished{status:401}` followed by the client holding its credential invalid (no further `http.started` until the token changes) | Credential rejection | Induced — real round trip |
| 6 | `operation.local_commit` with no later `http.started` for that write | Sync/transport failure | Induced — existing production test |
| 7 | `operation.local_commit` with no later UI-visible state change | Client state/rendering failure | **Not induced** — needs a live rendered UI, see Proof below |
| 8 | No Android `worker.started` in the window the push/schedule should have fired | OS scheduling or push-delivery issue | **Not induced** — needs a built Android install, see Proof below |

Read a row only after the end-to-end join above — the left-hand column names
a *pattern across the join*, not a single line to grep for.

## Proof: each row, induced and observed

Six of the eight rows below were reproduced against the real code in this
worktree, not a fixture; two were not (rows 7 and 8, each with its own
"not induced" entry stating why). No browser was involved in any of the
rows below — `client/web`'s `pnpm dev`/Vite dev server was started once
during this investigation but never produced a cited artefact: rows 1, 2,
3 and 5's own text below shows they bypass the browser entirely, rows 4
and 6 are `cargo test` runs, and row 7 is exactly the row that needed a
browser and could not get one (see its own entry). Two harnesses actually
produced the evidence cited:

- **Rows 1, 2, 3, 5** — a local `wrangler dev` running the real
  `hummingbird-authority` Worker, compiled to wasm, over real HTTP on
  `127.0.0.1:8787` (this stands in for `hb.twinion.net` for the reason in
  the banner above: production has not deployed this batch yet), hit from
  a native Rust process driving the real `hummingbird_core::Core::run_observed`
  sync engine directly — the same engine `client/ffi-web`/`client/ffi-mobile`
  drive, exercised here with no wasm boundary and no browser in the loop.
- **Rows 4, 6** — `cargo test -p hummingbird-ffi-web`, running two
  existing, already-passing tests against the real `TaskCoreCell` lock and
  capture path (`client/ffi-web/src/task_host.rs`) that are exactly the
  scenario each row names.

Every artefact cited here was redaction-checked (see **Redaction** below)
before being cited.

### Row 1 — client `http.started`, no server `request.received`

**Induced:** a real `hummingbird_core::Core::run_observed` cycle (the exact
engine `client/ffi-web`/`client/ffi-mobile` drive — driven here natively,
over real sockets, via `ReqwestSyncTransport`/`ReqwestMutationTransport`,
`server/scripts/smoke.sh`'s own "real `wrangler dev`, real HTTP" pattern)
captured one item, then attempted its drain against
`http://127.0.0.1:8799` — a port nothing listens on.

**Observed, client side** (`request_id: "diag712-row1-0"`):

```json
{"event":{"name":"http.started","payload":{"method":"POST","route":"/api/items"}}}
{"event":{"name":"http.finished","payload":{"method":"POST","route":"/api/items","status":null,"failure":{"kind":"unknown"}}}}
```

**Observed, authority side:** zero. `grep "diag712-row1" wrangler-tail.log`
against the real `hummingbird-authority` `wrangler dev` instance running
throughout this proof returns nothing for this cycle — the call never left
the device's network stack, exactly row 1's pattern. Note the client-side
signature (`status:null, failure:{"kind":"unknown"}`) is byte-identical to
row 3's — see "Known gaps" item 10 for why that pair alone is not
distinguishable without the authority-side join.

### Row 2 — server `request.received`, no server `request.finished`

**Induced:** a raw POST to `/api/items` on the real authority (`wrangler
dev`), with `Content-Length` declared larger than the bytes actually sent —
the DO logs `request.received` (before any awaited work, per
`server/worker/src/lib.rs`'s own comment on that ordering) and then genuinely
blocks inside `req.text().await`, since the declared body never arrives.

**Observed** (`cycle_id: "diag712-row2"`, `request_id: "diag712-row2-0"`),
checked *while the socket was still open* (3 seconds into a 6-second hold):

```json
{"event":{"name":"request.received","payload":{"method":"POST","route":"/api/items"}}}
```

— and only that line; no `request.finished` yet. Closing the socket
(ending the stall) immediately produced the second half:

```json
{"event":{"name":"request.finished","payload":{"method":"POST","route":"/api/items","status":500,"duration_ms":6013,"response_bytes":0,"token_id":null,"auth_result":"accepted"}}}
```

`duration_ms: 6013` matches the real 6-second hold — this is a genuine stall
inside the Durable Object's `fetch`, not a fast round trip misread. Read
`auth_result:"accepted"` on this 500 carefully: `classify_auth_result`
derives it from `path`+`status` alone (`server/authority/src/diagnostics.rs:135`,
its `(false, _) => AuthResult::Accepted` arm) — it is not a claim that
authentication actually ran or succeeded on this request, and here it
plainly didn't get far enough to.

### Row 3 — server `request.finished`, no client `http.finished`

**Induced:** the same real `Core::run_observed` engine as row 1, this time
against the real, reachable authority, but with its `reqwest::Client` built
with a 1ms request timeout — short enough that the client gives up before
it can read the response, even though the request reaches and is answered
by the real authority.

**Observed, client side** (`request_id: "diag712-row3-0"`):

```json
{"event":{"name":"http.finished","payload":{"method":"GET","route":"/api/sweep","status":null,"failure":{"kind":"unknown"}}}}
```

**Observed, authority side, the same `request_id`:**

```json
{"event":{"name":"request.finished","payload":{"method":"GET","route":"/api/sweep","status":200,"duration_ms":2,"response_bytes":1077,"token_id":"diag712-mine","auth_result":"accepted"}}}
```

The authority completed the request (`status: 200`) in 2ms; the client
never saw it. This is row 3 exactly — response path or client transport,
not an authority problem. Its client-side `http.finished` is
byte-identical to row 1's (see "Known gaps" item 10) — the authority-side
`request.finished` above is the only thing that tells the two apart.

### Row 4 — sync owns core while UI operation waits

**Induced:** the existing, real `TaskCoreCell` lock (`client/ffi-web/src/task_host.rs`
— the production core-ownership guard the SharedWorker actually calls, not
a mock of it) checked out under `CoreOwner::Sync`, then a project read
attempted while that checkout is still held.

**Observed** (`task_host::core_checkout_tests::a_project_read_started_behind_sync_is_busy_naming_sync`,
run directly — `cargo test -p hummingbird-ffi-web --lib -- a_project_read_started_behind_sync_is_busy_naming_sync`,
result `ok`):

```json
["core.wait_started", "core.busy"]
```

with the `core.busy` event's `owner: Some(CoreOwner::Sync)` — an interactive
read blocked behind sync, correctly naming the holder. (Contrast with the
"Known gaps" entry above: this `owner` is non-null here because a Rust
caller — this exact call site — always knows it; a TypeScript-originated
`core.busy` would still carry `owner: null` for the reason documented
there, and that is not this row's condition.)

### Row 5 — HTTP 401 followed by credential hold

**Induced:** the same real `Core::run_observed` engine, credential
deliberately wrong, against the real reachable authority.

**Observed, client side** (`request_id: "diag712-row5-0"`):

```json
{"event":{"name":"http.finished","payload":{"method":"GET","route":"/api/sweep","status":401,"failure":{"kind":"http","status":401}}}}
{"event":{"name":"sync.finished","payload":{"outcome":"credential_needed"}}}
```

`outcome: credential_needed` is `Core::apply_cycle_outcome` holding the
credential (`CoreCycleOutcome::Held` on the next attempt, proven by
`run_observed_holds_exactly_like_run_when_the_credential_is_held` in
`client/core/src/lib.rs`'s own suite) — the credential hold this row names.

**Observed, authority side, the same `request_id`:**

```json
{"event":{"name":"request.finished","payload":{"method":"GET","route":"/api/sweep","status":401,"duration_ms":1,"response_bytes":0,"token_id":null,"auth_result":"rejected"}}}
```

`token_id: null`/`auth_result: "rejected"` — no token was ever resolved, per
CLAUDE.md's "Credential blast radius" note on what a 401 log line does and
does not name.

### Row 6 — local commit, no later sync

**Induced:** the existing, real capture path
(`client/ffi-web/src/task_host.rs`'s `TaskHostCore::capture`) run once,
with no sync cycle ever attempted afterward.

**Observed** (`task_host::core_checkout_tests::a_successful_capture_emits_local_commit_before_finished_and_no_http_started`,
run directly, result `ok`):

```json
["operation.requested", "operation.local_commit", "operation.finished"]
```

with no `http.started` anywhere in the same journal — a real local commit
that this proof never sent anywhere, which is exactly what "no later sync"
looks like in an export. (This is also where the #739 gap in "Known gaps"
above bites hardest: even a real send afterward would share no id with this
operation, so the join a human would want to draw here does not exist yet
in the data.)

### Row 7 — local commit, no UI-visible change: not induced

**Not induced.** This row is inherently a rendered-DOM observation — "the
commit happened, the screen didn't change" — and the one environment
available for driving the real PWA (headless Chromium via Playwright,
`client/web`'s own `pnpm visual` toolchain) reliably crashed
(`Target crashed`, a real renderer crash, reproduced twice independently)
on this host: `vm_stat` showed under 100MB of free physical memory during
this session, with a second, unrelated Chrome session already resident.
Rows 1/3/5's proof above deliberately routes around a browser entirely
(the same `hummingbird_core::Core` a browser's SharedWorker calls, driven
natively) for exactly this reason — but row 7's condition has no
browser-free equivalent, since "UI-visible" is the whole claim. Whoever
next has a memory-unconstrained host: `pnpm dev`, capture through the real
`CaptureBox`, and diff the rendered Now/Triage state against the exported
journal's `operation.local_commit`.

### Row 8 — no Android `worker.started`: not induced

**Not induced.** Reproducing this needs a built and installed Android app
(`cargoNdkBuild`/`generateUniffiBindings`/`assembleDebug`, per this repo's
own gate) backgrounded across a real WorkManager scheduling window on a
real or emulated device — outside this slice's time budget once the
authority/PWA-native proof above and the memory-constrained browser
investigation for row 7 had run. A Pixel 6 Pro emulator (`emulator-5554`)
was booted and available but the app was never built against it. Whoever
next has the time: install the debug build, background the app past
`SyncWorker`'s scheduling window, and confirm the exported journal has a
gap where `worker.started` should be.

## Known gaps and divergences (read before trusting an export)

1. **The two exports' envelopes now agree.** Web's used to be
   `{"events","droppedCount"}` (camelCase, no envelope `schema_version`);
   #712 aligned it to Android's `{"schema_version","dropped_count","events"}`
   (`client/web/src/shell/diagnostics-download.ts`) — the side `protocol.ts`'s
   own cross-host snake_case rule already said should move. Per-event
   records were never divergent.
2. **`operation_id` does not cross the outbound-queue boundary — #739.**
   An `operation.*` span and its eventual `http.*` span for the same
   logical write share no id and different `cycle_id`s, so they are not
   programmatically joinable — only ordering (`operation.local_commit`
   before any later `http.started`) is provable, not identity. Acceptance
   criterion 7 in both #708 and #710 ("`operation.local_commit` before any
   `http.started` in the same operation") is therefore met by construction
   and by ordering, not by an id join.
3. **`core.busy`/`core.wait_started`/`core.acquired`'s `owner` can be
   `null` while the core is genuinely held.** See "Read a
   `DiagnosticEventV1` row" above — this is the single most misreadable
   field in the whole lane.
4. **`seq` is per-session, not per-cycle.** Repeated above because sorting
   an export any other way silently reorders it.
5. **`operation.stalled` is a still-running watchdog; `operation.abandoned`
   is terminal.** Two different names for two different things — do not
   read a `stalled` row as the end of the story.
6. **`NetworkChanged` fidelity differs per host — #740.** Android's
   `NetworkMonitor.kt` records transport (`cellular`/`wifi`/`vpn`/`other`/
   `none`) plus `internet`/`validated`/`metered`/`roaming`, never an IP.
   Web's is `{"online": bool}` only. #707's "unavailable fields as
   `unknown`" criterion was unsatisfiable against the closed
   `DiagnosticEvent` enum and was declined with that reason at the time.
7. **Android drains `core.*`/`operation.*` into the journal only around a
   `SyncWorker` run (before/after `core.run`) and on export — not on every
   UI action.** A span from an interactive action may sit unrecorded until
   the next sync or the next export. Also: `Core::run_observed` is not
   wired on Android, so Android's journal has no `Source::Core`
   `sync.*`/`http.*` rows at all — only the web PWA does.
8. **Redaction's forbidden-field list is hand-copied per host — #741.**
   `FORBIDDEN_FIELD_NAMES` is `#[cfg(test)]`-private in
   `server/domain/src/diagnostics.rs`, so nothing enforces the Kotlin/
   TypeScript scanners (wherever they exist) stay in sync with it by
   anything but review. A `DiagnosticEvent` variant addition is only half
   compiler-checked the same way: the `canonical` match forces a match arm,
   but nothing forces a fixture-array entry, and a missing one compiles and
   passes clean while silently skipping the redaction test.
9. **#742** collects three smaller leftovers from this batch: `cargo fmt`
   is not a usable gate under `client/`, a masked dead disjunct in
   `evictOverBudget`, and the visual gate's accessible-name pins matching by
   substring.
10. **A connection failure and a client-side timeout can log identically.**
    `classify_transport_error` (`client/core/src/diagnostics/failure.rs`)
    falls back to matching words like `"timeout"`/`"connect"` in
    `reqwest`'s error message when no HTTP status is present — but
    `reqwest` 0.12's `Display` for an error strips the underlying source
    chain, so a genuine connect-refused failure and a client request
    timeout can both surface as the same bare message and both classify
    as `FailureClass::Unknown`. Rows 1 and 3's proof below hit exactly
    this: two different real failures (no receiving host at all, versus a
    server that answered and a client that gave up first) produce the
    identical client-side signature `status:null,
    failure:{"kind":"unknown"}`. An operator cannot distinguish "never
    left the device" from "the device gave up on a real answer" from the
    client journal's `http.finished` row alone — the authority-side join
    is what tells them apart (row 1 has no authority-side line at all; row
    3 has a `request.finished` for the same `request_id`).

## Retention — the server side expires first

- **Device journal: 72 hours or 10 MiB, whichever comes first** (age-then-size
  eviction, both hosts — `client/web/src/worker/diagnostics-retention.ts`,
  `client/android/.../DiagnosticJournal.kt`'s `DEFAULT_MAX_AGE_MS`/
  `DEFAULT_MAX_SIZE_BYTES`).
- **Authority Workers Logs: 3 days on the Free plan, 7 days on Paid** — a
  Cloudflare platform limit, not something this repo's config sets
  (`server/worker/wrangler.toml`'s `[observability]` comment). Unlike the
  device journals, there is **no export** for these lines — evidence that
  must outlive this window has to be captured by hand (a `wrangler
  tail`/dashboard copy, pasted through the redaction check below) before it
  expires.
- **The server side expires first.** An incident exported more than 3 (or 7)
  days late can have a client journal with a `http.started` and *no*
  authority-side counterpart purely because the server's copy is gone — that
  absence must not be misread as row 1 ("failure before the authority"). If
  the client journal itself is more than 72 hours/10 MiB stale, the relevant
  rows may already be evicted on-device too, and the export will simply not
  contain them.

## Scope — what this lane does not do

- **No automatic upload.** Every export above is a manual, operator-initiated
  gesture on one device at a time. Nothing in this repo ships a journal
  anywhere on its own.
- **No mirror in the export.** The diagnostics journal and the offline item
  cache (the "mirror") are two separate stores with two separate downloads;
  an export never carries item content, titles, or the mirror's own data.
- **No timeout or locking change.** This lane observes the sync/core/HTTP
  lanes as they already behave; it changes none of their timeouts, retry
  policy, or locking. Making any of those changes is #704's job, not this
  slice's — this doc only gives #704 (or whoever debugs the next incident)
  the evidence to read.

## Redaction

Before anything from an export is pasted into this doc, an issue, or a PR,
it goes through the same forbidden-substring check the automated tests use
(`server/domain/src/diagnostics.rs`'s `FORBIDDEN_FIELD_NAMES` — item titles
are the likely leak, since `DiagnosticEvent`'s payloads are closed types
that structurally cannot carry one, but a title can still end up in a
pasted screenshot or a copied log line if an operator is not careful). No
artefact in this doc's proof section carries operator data — every
capture used a title created for this proof and named as such.

## An ADR was considered and declined

Six slices (#706–#711) shipped a cross-language wire contract (a closed
`DiagnosticEvent` enum, a shared envelope, a redaction rule, the
cross-language payload rule for the `core.*` quad) without any of them
opening an ADR, and this slice does not either. The reason: every decision
with lasting consequence in this lane already has a canonical, single-owner
home that is *more* precise than an ADR would be — `server/domain/src/diagnostics.rs`'s
own module header for the wire contract and the payload rule, and the six
PRs' review threads (#733–#738) for the alternatives-considered record an
ADR would otherwise carry. Writing an ADR now would either restate that
header (drifting the moment one of them is next amended) or under-describe
it. If a future change to this lane makes a decision that has no such
home — e.g. changing who owns `operation_id` across the queue boundary
(#739) in a way that trades off two real alternatives — that change is the
one that should open an ADR, not this doc.
