# hummingbird

Personal GTD-style task system. The task authority is moving from Linear
(org `twinion`, team `ION`) to an app-owned server (ADR-0008); the Linear
workspace stays the live working surface — and the skills below still target
it — until the owned stack is daily-usable. See `CONTEXT.md` for the domain
glossary.

## The sweeper

`sweep.py` is the one-way capture → Triage sweeper — one drain engine
over isolated adapters (Google Tasks fail-open, Gmail label-capture
fail-closed), the only built artifact of v0 capture. Stdlib-only Python,
one-shot, fired every 15 minutes by supercronic in a Fly worker. Its
invariants (one frozen namespace per source, create-in-authority-first
ordering, no Actions `schedule:`, no `[http_service]`) are load-bearing and
decided upstream; read `docs/sweeper.md` before touching any of them.
**Currently OFF (2026-08-08)** pending the authority move to the app-owned
server (ADR-0008); it retargets to `POST /api/items` when the owned stack is
daily-usable.

## The authority server

`server/` is the app-owned authority (ADR-0008/0009), its own Cargo
workspace: `domain` (the owned-schema types both sides will compile; the
client migrates onto them at S2/S3), `authority` (pure handler logic over a
sync `Sql` seam — plus an `Entropy` seam for token minting — fixture-tested
with rusqlite), `rules-engine` (fire-time evaluation of the ADR-0013
condition vocabulary, over the Event kind registry that lives in `domain`;
its `validate_rule` is wired into both `POST /api/rules` and
`PATCH /api/rules/:id` — a malformed condition is rejected at save with a
400, not just caught later at fire time; an unrecognized `event_kind` is
the one `RuleProblem` deliberately left unrejected there, since it is an
open registry key, not a closed vocabulary), and `worker` (the thin `workers-rs`
shim — one Worker, one SQLite-backed Durable Object). It carries the full
amended ADR-0009 schema plus the notification lane's
`rules`/`push_targets`/`deliveries` (14 tables,
`SCHEMA_VERSION 3`, ADR-0012/0013/0014), entity-level CAS writes (absolute
sets + `expected_version`, 409 carries the current entity, creates
idempotent by client id), the all-tables delta pull with `GET /api/sweep`
as its byte-identical backstop, bearer-token auth (sha256 at rest; scopes
`device`/`sweeper`/`ingest`; `/api/admin/tokens` gated by `ADMIN_SECRET`;
401 = bad credential, 403 = wrong scope or — for an `ingest` token, which is
bound to one alert source — a source mismatch, all empty-bodied), the
`POST /api/alerts` ingest upsert, and `POST`/`DELETE /api/push_targets`
(idempotent registration — a replay adopts a rotated `fcm_token` and
revives a revoked target, since neither event mints a new device id — and
individual, idempotent revocation). The notification lane's delivery leg
(#139) is `hummingbird_authority::deliver`: a **sync** function, not an
HTTP route — the real FCM send is necessarily async (the `workers-rs`
shim's `fetch`, on wasm32, where a sync trait cannot block on a future), so
`deliver` only decides and logs the transitions-only dedupe against
`deliveries` (`UNIQUE(alert_id, rule_id, generation, severity)`,
ADR-0012/0014) and hands the caller back exactly which live `push_targets`
rows to send to; the delivery row commits before the caller can possibly
begin sending, so a crash or retry never double-rings, and zero live
targets suppresses without logging rather than permanently burning the
transition. `hummingbird_authority::sweep_tick` (#138) is that async
caller's synchronous half: the DO alarm's repeat-tick evaluation, at the
`ALARM_INTERVAL_MS` interval (15 minutes, a readable `const` rather than a
buried literal, so #140 can warn when a rule duration is shorter than it).
Every enabled rule against every non-archived item, presented as a
synthetic `item_threshold` event; **every matching rule for one item is
collected before any write** — one `upsert_alert` call at the highest
matched severity (never one call per rule, which would deliver a rule at a
pre-ratchet severity and then re-fire once the ratchet moved the dedupe
generation), through the exact same upsert `POST /api/alerts` uses
(`item-threshold/v1`, `source_key` = `item:<id>`, ADR-0014's state-source
convention — occurrence identity lives in the alert's lifecycle, not the
tick) — then `deliver` runs once per matching rule against that one
already-ratcheted alert. Before minting, the sweep reads whatever alert
already exists under `item:<id>` and asks whether it is still live right
now: **still live** passes `raised_at: None` (keep the stored stamp, so an
unchanged match is a no-op landing on the same `deliver` dedupe generation
and staying quiet); **not live** — no row yet, or the row is
dismissed/resolved/expired — stamps `raised_at` fresh, which is what lets a
hand-dismissed alert ring again once its item next matches (ADR-0014's
"Live: how a settled alert rings again"). The sweep never writes to `items`
or `rules` — a tick is read-then-mint, never a write to what it read. The
worker shim (`hummingbird-authority-worker`) supplies the DO's `alarm()`
handler: it drives `sweep_tick`, reschedules the next tick unconditionally
(even if the tick itself errored, so one bad tick can't stop the clock),
and makes the actual FCM HTTP send for each `DeliveryOutcome::Logged`
target — `sweep_tick` only decides and hands back what to send, exactly as
`deliver` does one layer down. That send leg (#219) keeps the same split:
everything decidable is pure and natively tested in
`authority/src/fcm.rs` (the OAuth assertion's bytes, the FCM v1 message
body — ADR-0012's two tiers mapping onto Android's transport priority *and*
channel id — how to read a response, and the one write a send may make),
while `worker/src/fcm.rs` holds only `crypto.subtle` (RS256 over the
service-account key) and `fetch`, because `server/worker` has no test
harness of any kind and anything expressed there is untested by
construction. The credential is the `FCM_SERVICE_ACCOUNT` **Worker
secret** — never a var, never in `wrangler.toml`, and unset (every
`wrangler dev`, and CI) the lane fails closed: rules still evaluate,
deliveries still log, and each unsendable one is a `console_error!`.
**Policy: no retry, ever.** `deliver` commits the claim row before the send
is attempted, so a retry that re-sent would be the double-ring ADR-0012
says destroys trust in the channel; a failure is logged and dropped. The
one exception is FCM's `UNREGISTERED` error code, which revokes that
`push_targets` row — and it is matched on the `FcmError` detail alone,
never on the 404 status, because a wrong `project_id` 404s identically and
would otherwise revoke every device the operator owns. ADR-0014's
**resolution pass** (#217) is the tick's second phase, and the reason the
lane can ever go quiet on its own: it iterates live `item-threshold/v1`
alerts — not items, since an item-side scan only reaches the alerts whose
items it still sees, precisely the set that does not need resolving — and
stamps `resolved_at` on each one the tick did not match. Phase one records
the `source_key` of everything it minted, phase two resolves exactly the
complement, so **the two phases partition the alert set** and neither can
write a row the other touched; that is what makes ADR-0014's four triggers
one test rather than four (done is skipped by phase one, archived is
excluded by `load_live_items`, deleted is absent from `items`, and no
longer matching yields no verdict). Only *live* alerts are considered, so
the pass is idempotent — a resolved alert is not live next tick, and its
stamp never creeps. Independently, **the sweep never clears
`resolved_at`**: it carries the stored value back through `upsert_alert`
rather than passing `None`, because that handler sets source-owned fields
*absolutely* and would otherwise erase the pass's stamp one tick later.
What supersedes a resolution is a later raise — `raised_at` overtakes the
stamp, the alert is live again and rings, and the stamp stays legible
underneath. Note that `done` is a *resolution* boundary only: #138's
evaluation boundary is still `archived_at` alone. Still no production deploy
(that is #95's human gate H3) — `wrangler dev` + `server/scripts/smoke.sh`
locally, `.github/workflows/server.yml` in CI. **The test recipe is shared,
not copied** (#229): `server-test.yml` is a `workflow_call`-only workflow
holding clippy / native fixture tests / wasm32 build / `smoke.sh`, and both
`server.yml` (pull requests only) and `deploy-server.yml` (`main`, `wrangler
deploy` behind `needs: test`) call it, so the gate a PR sees and the gate the
deploy passes cannot drift. `server.yml` carries no `push:` trigger for
exactly that reason — `main` is gated by the deploy workflow's own test job,
and a second copy would run the same 30-minute job concurrently for no extra
signal. `deploy-server.yml` carries no `schedule:` — the
DO's own `alarm()` owns the cadence — and no `ADMIN_SECRET`/
`FCM_SERVICE_ACCOUNT`; it is red-by-design until H3 creates the Workers
project.

## The client sync engine

`client/core/src/sync/` is the device half of the owned stack (ADR-0008), and
the largest thing in `client/`: `adapter`/`transport` are the read side (the
normal pull is the delta since the mirror's own version; `GET /api/sweep` is
the correctness backstop, on app open and daily), `write/` is its mirror
image on the write side (CAS mutations, rebase-on-409, deterministic ids,
the error taxonomy), `mirror` is the local read model where absence demotes
rather than deletes (ADR-0003), `queue` is the durable FIFO plus its
dead-letter journal, and `cycle` is ADR-0007's one cycle — drain, then pull,
in that order, every time, with jittered backoff capped at five minutes.
**Durability belongs to the cycle, not the queue**: capture code calls
`SyncCycle::enqueue`/`run`, never `OutboundQueue::enqueue`/`drain` directly,
because only the cycle pairs each mutation with the snapshot write that
makes it durable before anything is sent. Clock, jitter and access token are
caller-injected on every call — bare `wasm32-unknown-unknown` has no clock or
RNG that does not panic. There is no `docs/sync.md`; the map is the module
docs in `client/core/src/sync/mod.rs` and each submodule's own header.

**A 409 is a three-way decision, not two** (#163/#164). `write/rebase.rs`'s
`decide` diffs every touched field against both the client's `base` and the
409's `current`: a field that moved to some third value is a `Collision`,
naming every colliding field; a patch whose touched fields *all* already
hold their intended value is `RebaseDecision::Achieved` — this write in fact
already landed (a crash swallowed the ack), so it is **not resent at all**,
the 409's carried entity becomes the outcome, and the authority never sees
the replay; only `Safe` — at least one field still needing reapplying,
nothing colliding — reissues the identical touched-field intent at the new
version. `write/adapter.rs`'s `patch_with_rebase` is a bounded attempt loop
(`MAX_ATTEMPTS = 3`: the original send, the retry a first `Safe` 409 earns,
and one further retry for a `Safe` second), each attempt diffed against the
`current` it was actually rebased onto rather than the original `base`. A
third attempt that is *still* disjoint is repeated churn, not a collision,
and terminates as `WriteError::Contention { current }` → the queue's
`DeadLetterReason::Contention` — carrying the server entity so the journal
still has material, because the alternative is a `Conflict` with an empty
field list masquerading as a real one, showing the reader nothing to act on.

`Core` (`client/core/src/lib.rs`) is the one door onto all of that, and it
has exactly **three mutation entry points**: `Core::capture` (a create,
whose `title` goes through `capture::parse_seam` — #110/#42's named no-op —
and reaches the mutation verbatim regardless), `Core::act` (S11's closed
`ItemAction` vocabulary: start / complete / block / cancel, where cancel
sets `archived_at` and never a stage, because the owned schema has no
"canceled"), and `Core::triage` (S13's `TriageDestination` + `TriagePatch`:
a multi-field triage is exactly ONE queued CAS `PATCH`, never one per
field, so a 409 rebases or dead-letters the whole edit together). **All
three enqueue through `SyncCycle::enqueue` and none of them may reach
`OutboundQueue::enqueue`** — the durability rule above is not per-caller
advice, it is what makes an offline capture, act or triage survive at all.
All three take a caller-minted `seed` (deterministic id, same no-clock/no-RNG
reasoning). The reads are `frontier` / `triage_inbox` / `blocked` /
`steps_for` / `projects`; the three item queries are each a filter over one
shared `overlaid_items` view, while `steps_for` and `projects` (over
`SyncMirror::all_projects`) read the mirror directly — no mutation entry
point mints a Step or a Project, so there is nothing optimistic to overlay
there.

**The overlay is one representation, not one per mutation kind.**
`overlay_from_queue` rebuilds it at `Core::init` from whatever the durable
queue still holds — `MutationIntent::Create` through `item_from_create`,
`MutationIntent::Patch` through `apply_item_patch` (the same absolute-value
field overwrite the wire sends) — so a capture, an act or a triage made
offline and then reloaded is still readable, still `is_pending`, rather
than vanishing until the next successful cycle. A queue entry that no
longer projects is an `Err`, never a silently dropped overlay entry: going
overlay-blind would tell a reader nothing is pending while something still
is. It is keyed one entry per item id in FIFO order (last enqueued wins),
which leaves the narrow `entry_id` gap `Core::act`'s own doc records —
flagged there, not fixed.

`client/core/src/rank.rs` is the other top-level module beside `Core`, and
it is no part of the sync engine: `rank()` (#162) is
`/next-up-personal`'s six ranking steps made pure, so a device can pick
"what to do right now" offline — context hard-filter, overdue / due-today,
In Progress bias, priority, energy/size fit plus the 30-minute calendar
nudge, then oldest-first. **No I/O, no credentials, no clock read**: "now"
arrives as the caller-injected `Now`, carried in two shapes (naive-local, in
exactly `Item::deadline`'s own spelling, and the same instant as epoch ms)
because deriving one from the other would need a time zone this crate has no
business guessing. It speaks only `hummingbird_domain::Item` and
`calendar::EventRecord` — no Linear vocabulary and deliberately no
translation layer onto either — and consumes `calendar::query`'s
`is_actionable`, so a cancelled future instance never fires the nudge, the
same invariant `query.rs` documents for "Next". Every candidate carries
every `ReasonCode` that applied, in step order, so #116's skill layer can
cite the actual decisive rule rather than a step index. The total order ends
`created_at` then `id`, with nothing left to chance, and
`the_same_snapshot_ranked_twice_is_byte_identical` is what pins that a
repeat call is byte-identical. Nothing consumes it yet — it crosses no FFI
seam; #116 is its caller.

## The web worker layer

`client/web/src/worker/` is where the device half meets the browser: **one
`SharedWorker` per origin, N views** (ADR-0010, #126). `core.worker.ts` is
the shim only — it loads the wasm core with a dynamic `import()`, wires
`ports.ts`'s `PortRegistry` (the port list, the per-port `ready`/`error`
handshake, the broadcast fan-out; a port that connects before the core
exists is queued, never dropped), and owns **ADR-0007's single 60-second
interval for the whole origin**, because a timer in a per-view hook
multiplies with tab count and blows the ADR's ~60 req/hr budget. Everything
decidable is a sibling pure module a vitest (node) test can execute:
`dispatch.ts` (cadence / task / calendar routing, plus the app-open sweep,
which fires on the first `pushTaskApiKey` — never at activation, when no
credential is known yet), `request-router.ts`, `task-worker.ts` and
`calendar-worker.ts` (the two wasm hosts' own serial queues and JSON
parsing), `visibility-tracker.ts` (a `SharedWorker` has no `document`, so
each view reports its own `document.hidden`). The view side is
`store/protocol.ts` (the whole wire contract, push-only worker→view),
`store/worker-client.ts` (the only translation of that protocol into store
writes), `shell/useSyncWiring.ts` and `shell/useTaskTokenWiring.ts` (#106's
device token: entry, rest, re-prompt — the key crosses into the core and is
never read back out through any response), and `shell/sync-status.ts` (the
staleness readout; an outcome that did not run must never read as success).

**That one interval now runs behind an in-flight guard, and trigger identity
survives it.** `worker/sync-run-guard.ts` (#184) wraps the `run` sink
`core.worker.ts` hands `createSyncCadence` — not the cadence, and not the
serial queue, whose own abandon-on-timeout is an unrelated fix that must
keep working — so at most one `runSync` is in flight for the whole origin.
Triggers arriving during a run coalesce into exactly ONE pending follow-up,
which starts the instant the in-flight one resolves; and because a
`runSync` whose promise never settles would otherwise wedge the cadence
forever, the guard releases the slot after its own bound, `releaseMs`,
passed in as `TASK_REQUEST_TIMEOUT_MS` — the same "how long is too long"
the underlying task queue already answers, never a second independent
number. A generation counter is what makes that release safe: a straggler's
late `.finally`, arriving after the bound already handed the slot to a new
run, is stale and can never free that *other* run's slot. Which of two
triggers survives in the single pending slot is
`shell/sync-cadence.ts`'s `mergePendingSyncTrigger`, never bare last-wins,
because identity is read downstream of the guard (`forceFullSweep`,
`toCoreTrigger`): `open` (3) > `reconnect` | `manual` (2) > `focus` (1) >
`timer` (0). A pending `open` overwritten by a later trigger would drop
ADR-0008's app-open full-sweep backstop for the rest of the worker's
lifetime (`dispatch.ts` fires it once), and a pending `reconnect`/`manual`
overwritten by a `focus` or `timer` would silently demote a user-facing
backoff reset — the outage-recovery path the precedence exists for. **The
guard module imports nothing, and must stay that way**: it sits in
`core.worker.ts`'s static import graph, which may never acquire a top-level
`await`.

`toCoreTrigger` maps `"focus"` onto the core's `"timer"` spelling, not
`"user"` (#190). A focus event says a window came forward — an ambient
signal, not the gesture ADR-0007's "backoff is reset by any user-facing
trigger" is about; only `open` / `reconnect` / `manual` are, and they still
reset it. The cadence itself is unchanged and `useSyncWiring.ts` still
forwards every focus, but outside backoff a focus behaves exactly as before
and during backoff it lands as a cycle the core declines, so alt-tabbing at
any rate can no longer stretch an outage's request rate past the backoff
schedule. An interpretation of ADR-0007 recorded in the code, not an
amendment to the ADR.

The protocol now carries the whole read-and-act surface: view→worker
`capture` / `act` / `triage` and the reads `getFrontier` /
`getTriageInbox` / `getBlocked` / `getSteps` / `getProjects` / `isPending`;
worker→view `captureResult` / `actResult` / `triageResult` plus the
`frontier` / `triageInbox` / `blocked` / `steps` / `projects` /
`isPendingResult` pushes. A frontier or blocked entry is a
`FrontierItemDTO` — `ffi-web/src/task_host.rs` flattens
`hummingbird_domain::Item` and adds the computed `pending` flag, stamped in
exactly one place (`TaskHostCore::with_pending`, applied by `frontier()`,
`triage_inbox()` and `blocked()`) so the answer cannot drift between call
sites; `pending` is a read-time fact about the overlay, never a schema
column. Every wire string is resolved by name before the seam
(`parse_action`, `parse_destination`, `Stage::parse`, `Size::parse`,
`Energy::parse`), so an unrecognised name fails without ever reaching
`Core`. On an `ok` result `store/worker-client.ts` re-requests the affected
queries itself — `actResult` re-reads frontier, blocked and that item's
`isPending`; `triageResult` re-reads the triage inbox and the frontier —
which is what makes a mutation taken offline visible immediately, without
waiting for a cycle.

**The invariant: no top-level `await` may enter `core.worker.ts`'s static
import graph.** `vite-plugin-top-level-await` wraps such a module in an
async IIFE, which pushes the `self.onconnect` assignment past the first
turn — and a `connect` event has no platform buffering, so the connect
queued by the very view that STARTS the SharedWorker is silently dropped and
that view hangs on "Loading core…" forever. This is why the wasm module is
imported dynamically inside an async IIFE. Adding a static import to that
file, or a top-level `await` to anything it imports, re-breaks it;
`worker/sync-timer-ownership.test.ts` pins what it can from the source text,
but the real proof is the built bundle (zero `await` at function-depth ≤ 1
before `self.onconnect =`).

## The screens and shell layer

`client/web/src/screens/` and `client/web/src/shell/` are the read-and-act
surface S10–S13 built over that protocol — Now renders the frontier, its
project groups and item detail; Triage renders the capture box and the
inbox. **Everything decidable is a pure `screens/*.ts` module a vitest
(node) test can execute, and the `.tsx` components only thread React state
through them**, the same split `worker/*` already uses: `frontier-order.ts`
(priority rank, then deadline, then id) over `priority.ts` (Linear's
inverted, holed 0..4 encoding survives in the schema, so nothing sorts or
renders the raw number — ADR-0002 leaves ranking to consumers),
`frontier-groups.ts` (group by `projectId`, names resolved from the
`getProjects` answer, unassigned last), `urgency.ts` (CONTEXT.md's
read-time urgency, computed fresh per render and never written back onto a
`TaskItemDTO`; `deadlineSortKey` is the TS twin of the domain crate's, and
a day-only deadline resolves to `T23:59` local wall clock, never a UTC
instant), `blocked-reason.ts`, `capture-validation.ts` (an empty or
whitespace-only capture is refused here, because `Core::capture` has no
opinion of its own and would enqueue it), `triage-order.ts` (capture order,
by `createdAt`, which reads the same before and after the overlay clears),
`item-actions.ts` (which actions a stage offers, the optimistic
`applyItemAction` projection, and `resolveFallbackPending` for an item that
has left every live query) and `triage-form.ts` (which drafted fields are
actually changes — `null` means "leave this field alone", never an empty
string sent as an edit).

The `shell/use*Wiring` hooks are thin glue and **own no clock**: each
re-requests its queries once the core is ready and again on every
`TaskState.syncOutcomeSeq` bump, because ADR-0007's single 60-second
interval in the SharedWorker is the only timer the origin gets.
`useCaptureWiring`'s exported `submitCaptureRequest` posts `capture` and
then `getTriageInbox` right behind it — `task-worker.ts`'s serial queue
makes the second land only after `Core::capture` has already returned — so
the optimistic item is on screen before any network call, which the
per-cycle effect alone can never do (it fires after a cycle, the exact
inverse). `capture-hotkey.ts` is deliberately DOM-free, the caller
extracting the facts from the real `KeyboardEvent`: plain "c", never with a
modifier, never while an editable control has focus or an IME composition
is in progress.

**Component tests are the gate on that threading, and they exist because
typecheck cannot see that something has no caller.** A pure module, a
`use*` hook or a prop that is exported, unit-tested and never wired
compiles clean and passes clean — which is why three of the S10–S13 PRs
were rejected on first review for shipping UI state with no reader (the
worst, `disabled={item.pending}` on a permanently-frozen flag, survived two
rounds). `client/web/vitest.config.ts` keeps `environment: "node"` as the
DEFAULT — the `worker/*` tests assert against a `SharedWorker`-shaped world
with no `document`, and handing them a jsdom global would let a test pass
on a `document` the real runtime lacks. A `*.test.tsx` opts into jsdom per
file with a `// @vitest-environment jsdom` docblock. Everything mounts
through `src/test/component.tsx`, which registers the `afterEach(cleanup)`
RTL would otherwise skip (this repo runs vitest without `globals: true`,
and unclean teardown reads as a flaky assertion) and carries the
`itemDTO`/`taskState` builders so a test states only the fields it is
about. Pure-module tests still cover the deciding logic; the component
tests cover the wiring between them, and neither replaces the other.

**The visual gate is Playwright, and it is local-only.** `pnpm visual` in
`client/web` drives three real viewports (1440 / 1024 / 768 — the third is
the wrap point of `screens/layout.tsx`'s `TwoColumn`, which uses no media
query) across both themes, writing captures to `visual/.captures/` for
review. There is no committed golden and no pixel diff: what it *fails* on
is the machine-decidable subset — horizontal overflow, an unresolved brand
token, a theme switch that never reaches the page. It is deliberately not
in `.github/workflows/client.yml` (`pnpm typecheck` already rebuilds the
wasm core and is that workflow's slow step), and it needs a one-time
`pnpm exec playwright install chromium`. Never `chrome --headless
--screenshot` for this UI — the viewport renders wrong. The registry
`/wrapup` reads is `docs/SURFACES.md`.

## The design system

The UI brand is the "Hummingbird Design System" project on claude.ai/design;
`.claude/skills/hummingbird-design/` is its repo-local mirror (tokens, the
16-component library, the web UI kit — sync record and what was deliberately
omitted in that directory's `github.md`). **All frontend/UI work must use
it: invoke `/hummingbird-design` before styling anything.** The web app
consumes it via `client/web/src/design/` (a copy of the tokens, with
`fonts.css` swapped to self-hosted `@font-face` because the production CSP
allows no Google Fonts) and `client/web/src/styles.css` (maps tokens onto
Tailwind utilities, dark mode on `[data-theme="dark"]`). When the design
project changes: re-pull the mirror first, then re-copy tokens into
`client/web/src/design/`.

## Agent skills

### Issue tracker

Issues live as GitHub issues in `JddAndrewLauren/hummingbird`, driven via the `gh` CLI;
the wayfinder map is issue #1. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary — `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`,
`wontfix`, plus the non-triage `plan` role. All exist on the tracker.
See `docs/agents/triage-labels.md`.

### Microtasking

`/microtask <issue-id>` breaks one already-selected, stalled Linear issue into a checklist of
~2–5-minute Steps written into its body. See `.claude/skills/microtask/SKILL.md`.

### next-up-personal

`/next-up-personal` picks what to do right now from the Linear workspace — one ranked top
pick plus a health footer — and `/next-up-personal <issue-id>` hands one `agent`-labelled
issue to an agent. See `.claude/skills/next-up-personal/SKILL.md`.

### Domain docs

Single-context — root `CONTEXT.md` glossary plus `docs/adr/`.
See `docs/agents/domain.md`.
