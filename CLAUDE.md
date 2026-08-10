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
its `validate_rule` exists but is not yet wired into `POST /api/rules` —
`authority` does not depend on the crate — so a malformed condition is
currently caught only at fire time), and `worker` (the thin `workers-rs`
shim — one Worker, one SQLite-backed Durable Object). It carries the full
amended ADR-0009 schema plus the notification lane's
`rules`/`push_targets`/`deliveries` (14 tables,
`SCHEMA_VERSION 3`, ADR-0012/0013/0014), entity-level CAS writes (absolute
sets + `expected_version`, 409 carries the current entity, creates
idempotent by client id), the all-tables delta pull with `GET /api/sweep`
as its byte-identical backstop, bearer-token auth (sha256 at rest; scopes
`device`/`sweeper`/`ingest`; `/api/admin/tokens` gated by `ADMIN_SECRET`;
401 = bad credential, 403 = wrong scope or — for an `ingest` token, which is
bound to one alert source — a source mismatch, all empty-bodied), and the
`POST /api/alerts` ingest upsert. Still no production deploy (that is #95's
human gate H3) — `wrangler dev` + `server/scripts/smoke.sh` locally,
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
