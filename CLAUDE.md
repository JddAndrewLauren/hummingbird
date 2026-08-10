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
synthetic `item_threshold` event; each match mints/ratchets the alert
through the exact same upsert `POST /api/alerts` uses (`item-threshold/v1`,
`source_key` = `item:<id>`, ADR-0014's state-source convention — occurrence
identity lives in the alert's lifecycle, not the tick), then calls
`deliver` once per matching rule. A repeat tick that still matches is
therefore just another call into #139's frozen dedupe key, not a new
mechanism: the re-raise keeps the alert's `raised_at`, so `deliver` finds
the same `(alert_id, rule_id, generation, severity)` already logged and
suppresses it. The sweep never writes to `items` or `rules` — a tick is
read-then-mint, never a write to what it read. The worker shim
(`hummingbird-authority-worker`) supplies the DO's `alarm()` handler:
it drives `sweep_tick`, reschedules the next tick unconditionally (even if
the tick itself errored, so one bad tick can't stop the clock), and is
where a future issue wires the actual FCM HTTP send for each
`DeliveryOutcome::Logged` target — `sweep_tick` only decides and hands back
what to send, exactly as `deliver` does one layer down. Not yet built:
ADR-0014's separate "resolution pass" (stamping `resolved_at` on an
`item-threshold/v1` alert once its item is done, archived, deleted, or no
longer matches) — until it lands, an alert minted for an item that stops
matching stays live until hand-acked. Still no production deploy (that is
#95's human gate H3) — `wrangler dev` + `server/scripts/smoke.sh` locally,
`.github/workflows/server.yml` in CI.

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
