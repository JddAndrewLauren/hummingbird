# ADR-0025: Decisions sink to the core; rendering stays per-client

**Status:** accepted · 2026-08-14 · **amended 2026-08-15 (#499):** M1-1's
probe fixed the web-side mechanism — a second, main-thread instantiation of
the existing `hummingbird_ffi_web` wasm module, exposing free
`#[wasm_bindgen]` functions — and recorded what does *not* sink in M1. See
[The web seam, fixed by M1-1's probe](#the-web-seam-fixed-by-m1-1s-probe)
below. **Amended 2026-08-15 (#500):** M1-2 sank the capture decision set
(urgency, the deadline-field grammar, `vocabulary.rs`, and
`capture_meta_problems`) and found one more case the verdict table did not
yet cover — a vocabulary's *value list itself* (`field-vocabulary.ts`'s
`SIZE_OPTIONS`/`ENERGY_OPTIONS`/`CONTEXTS`), read as a plain array at
React-render time by a component reachable, transitively, from `main.tsx`'s
static `import { App }`. A `const` computed by calling the wasm seam
executes at MODULE EVALUATION — which for that whole import graph happens
before `initDecisions()` is ever awaited — so it would throw the "used
before ready" guard on every page load; `urgency.ts`/`deadline-parts.ts`/
capture-meta's decision half sink cleanly because every one of *their*
exports is a function, called only from event handlers and render bodies,
never at module-evaluation time. `field-vocabulary.ts` therefore keeps
hand-written arrays, now pinned against `hummingbird_core::decisions::
vocabulary`'s real, seam-exposed functions by `field-vocabulary.test.ts`
rather than sunk at runtime — see that module's own header for the full
argument and #500's PR description for the trade-off as it was decided.
**Amended 2026-08-15 (#501):** M1-3 sank the frontier's ordering, grouping
and faceting (`decisions::frontier`, replacing `client/core/src/task/
query.rs`'s `by_priority_then_due` — ADR-0021 decision 1's one spelling)
and the combined Now/Triage queue (`decisions::queue`). The same
module-evaluation-order constraint #500 found applies again to three more
constants — `frontier-columns.ts`'s `FRONTIER_AXES`/`DEFAULT_FRONTIER_AXIS`
and `frontier-facets.ts`'s `FACETS`/`SIZES`/`ENERGIES`, all read at
React-render time by components statically reachable from `main.tsx` — so
those five stay literal TS in `seam.ts`, pinned against the core by
`seam.test.ts` rather than sunk at runtime, the same shape as
`field-vocabulary.ts`'s own arrays. `frontier-facets.ts`'s `SIZES`/
`ENERGIES` was the one surviving unpinned vocabulary copy the #500 review
flagged; it is pinned now, not sunk further, for the reason just stated.
**Amended 2026-08-18 (#533):** M4's gating probe fixed the mechanism for the
standing-question panes, whose obstacle is unique — panes are **civil-date
reasoning** and the core owns no tzdb. The answer is a **two-phase zone
bridge**: the core names every `(zone, civil-date)` fact it needs, the host
resolves them (`Intl` on web, `java.time` on Android), and the core ranks
against the result; a zone the host cannot resolve crosses back **absent**,
and what an absent fact means stays a core decision. One pane (waste) was
sunk with it, and the verdict on whether two phases are tolerable gates
#534 and the rest of the pane lane. See [The zone bridge, fixed by M4's
probe](#the-zone-bridge-fixed-by-m4s-probe) below.
**Amended 2026-08-18 (#538):** M4's probe sank the *skills runner lane*'s
decision half — `decisions::skills` (`envelope`, `run`, `grill`, `decline`,
`args`) — and rewired `client/web/src/skills/`'s six rule modules onto it in
the same slice, so the phone could drive `POST /api/skills/run` without a
second copy of any of it. Three things this ADR had not had to say before,
each now a verdict-table row: **line splitting stays per-client** (a
byte-level stream reader is a platform fact, not a decision — the web's
`TextDecoder` + `takeLines`, okio's `readUtf8Line` on Android); **the
transport stays per-client** (`run-skill.ts`, `route-run.ts` and the Kotlin
`SkillRunner`, none of which decides anything); and **three decline
constants stay literal TS** — `NO_TOKEN`, `NO_TERMINAL_LINE`,
`OUTSIDE_SCHEMA` — for exactly the module-evaluation-order reason #500 and
#501 already recorded, pinned against the core by `seam.test.ts`. This slice
also invented the repo's first **cross-language shared fixture**
(`client/core/tests/fixtures/skills-run-bodies.json`, read by Rust,
TypeScript and the Android instrumented suite), and took the per-gesture
free-door carve-out one step further on the mobile seam: the reducers are
per-*event* doors, admissible because the events arrive seconds apart on a
stream and the alternative is a Kotlin copy of the reducer. #274's
routing/registry/memo and `microtask-affordance.ts` are deliberately not
sunk here — they are #539's.
**Amended 2026-08-18 (#540):** M4 sank the rules-editor decision set —
`decisions::rules::{operators,duration,validity,deadline,editor,backtest}`
— which retires both drifts this ADR's own verdict table had recorded as
known debt (`rules/backtest.ts:52`, `rules/deadline-picker.ts:32`) and
the operator table, the duration grammar, the validity read and the
field/widget cascade alongside them. Two findings worth the record.
**First:** `client/core` can depend on `hummingbird-rules-engine`. That
crate takes `hummingbird-domain` and `serde_json` and nothing else, so it
crosses the `wasm32-unknown-unknown` build and `client/next-up`'s
`default-features = false` build untouched — `backtest.ts`'s header had
claimed the opposite ("a native-only crate this build has no wasm path
to"), and that claim was rewritten, not annotated. **Second:** the
strongest retirement — calling `evaluate_rule` wholesale — is not
available to a *client-side* backtest, because that function takes a
single `now` and a client reads two frames of one instant (`occurred_at`
in UTC, `deadline`/`scheduled_date` in device-local civil time). The sink
took the other arm: core-side evaluation assembled from the engine's own
`Operator` plus `hummingbird_domain::deadline`'s primitives, so every
primitive still has exactly one owner while the two-frame assembly lives
in `decisions::rules::backtest`. The frames are named at the boundary
(`BacktestClock`), never inferred. The M1 module-evaluation-order
constraint did not bite here: every export of the seven `screens/rules/`
modules is a function called from an event handler or a render body.
**Amended 2026-08-18 (#535):** M4 sank the Settings screen's sync-status
readout and the dead-letter heading — `decisions::settings::{
sync_outcome_class, is_informative_sync_outcome, relative_age,
sync_status_tone, sync_status_label, sync_status_tone_word,
dead_letter_heading}` — and rewired `shell/sync-status.ts` onto them in the
same slice, plus exposed the same set through `client/ffi-mobile`'s free
`sync_status_summary`/`dead_letter_heading`/`is_informative_sync_outcome`
doors and a `MobileTaskHost::bindings`/`set_binding`/`dead_letters` read/write
trio mirroring `ffi-web::task_host`'s DTOs. Two carve-outs this slice
introduced, both now in the verdict table below: `isInformativeSyncOutcome`
stays a literal TS copy in its own module
(`shell/sync-outcome-informative.ts`) for a **new** kind of reason — not
M1's module-evaluation-order constraint, but a worker/main-thread
static-import-graph separation (ADR-0010): `worker/ports.ts` needs the
identical predicate and runs inside `core.worker.ts`'s own script
evaluation, which must never statically reach the seam
(`worker-import-graph.test.ts`'s gate) — and any static import anywhere in
a file pulls that file's *whole* graph in, regardless of which export is
used, so the predicate could not stay in `sync-status.ts` once that file
imported the seam for its other functions. `ThemePreference`/
`resolveDarkTheme` stay per-client on `frontier-prefs.ts`'s existing
"view prefs" verdict, widened here to cover a theme choice explicitly, not
only a frontier grouping/facet preference.
**Amended 2026-08-18 (#534):** M4 sank the remaining seven standing-question
panes — the status four (kimi/github/uptime/reachability) and the now three
(race/weekend/vacation) — growing `decisions::panes::SUNK` from waste alone
to all eight. Three findings worth the record, beyond the per-pane verdict
rows in [What does not sink with the panes, continued](#what-does-not-sink-with-the-panes-continued-534).
**First**, the facts-union question #533 deliberately left open is settled:
each pane's *facts* still cross on its own seam export (`kimi_facts_json`,
`github_facts_json`, …) rather than a single tagged union hung off
`RankedPaneRecord` — seven real arms made the union's shape obvious enough
to write, but no caller ever needs a pane's full facts and its rank in one
call (the shell reads `PaneAnswerCore` for ranking and a pane's own facts
only inside that pane's own `Expanded` render), so a union would cost a
type nobody was blocked on. **Second**, a new zone-bridge case:
`weekend.rs`/`vacation.rs` both need "the reader's own zone" rather than a
payload-carried IANA name, so [`zone::DEVICE_ZONE`](../../client/core/src/decisions/panes/zone.rs)
is a sentinel `zone` string the two-phase crossing already carried without
a new `ZoneQuery` variant — `zone-bridge.ts` is the one place it is given
meaning (`Intl.DateTimeFormat().resolvedOptions().timeZone`). **Third**, a
harder version of the M1 module-evaluation-order constraint: `weekend.ts`'s
`weekendWindow` is called at `describe`-body top level by `weekend.test.ts`
(before any `it()` runs), which executes during vitest's synchronous test
*collection* — before `wasm-setup.ts`'s `beforeAll` resolves
`initDecisions()`. `weekendWindow`/`weekendBand`/`weekendWithinBand`
therefore stay literal TS, pinned against `weekend.rs`'s own
`weekend_window`/`weekend_band`/`weekend_within_band` by
`weekend-window.shared.test.ts` rather than called through the seam at
runtime — `field-vocabulary.ts`'s precedent, for a collection-order trap
rather than a module-evaluation one. `weekendAnswer` itself (called only
inside `it()` bodies) does cross.
**Amended 2026-08-18 (#539):** M4 decided what #538 deliberately left open,
with the real second caller in hand — the Grill takeover and the microtask
affordance landing on the phone. Sunk: `decisions::skills::affordance`
(which gesture an item's steps make legal, and `live_undone_steps` behind
it), `decisions::skills::review` (whether confirming a verdict strands a
live plan or demotes the item off the frontier, out of
`screens/grill-review.ts`), and `decisions::skills::backend` (the
degrade-to-Auto rule out of `backend-selection.ts`'s
`readBackendSelection`, and the tier fallback out of
`backend-registry.ts`'s `fallbackEntry`). **Not sunk, and now
verdict-table rows rather than an open question:** the backend registry's
own *data* — label, model, endpoint, timeout — which is configuration two
clients may render differently without either being wrong; and
`route-plan.ts`/`reachability-memo.ts`, which stayed per-client because
Android's runner reaches one backend at a time under a device preference
and has no Auto sequence to plan, so sinking a router with a single live
caller would be shaping the rule from one arm — the trap #533 named and
#534 honoured. One finding worth the record: sinking
`microtaskAffordance` took the last caller away from
`microtask-affordance.ts`'s `liveUndoneSteps`, leaving an exported, tested
TS copy of a rule the core now owns. This ADR's no-hand-copied-decisions
rule covers a copy with no caller too, so it was **deleted** at #565's
review rather than kept as a helper a future caller could re-copy from.
**Amended 2026-08-18 (#564):** the Android calendar-connect slice ruled on
`client/web/src/calendar/`'s seven modules, and the ruling is a **split**
rather than either arm #564 originally offered. Sunk: `selection.ts`'s
derivation half — `effectiveSelection`, `effectiveCalendarIds`,
`acceptSelectionChange`, `unavailableSelectedIds`, `toggleCalendarId` —
plus `connection.ts`'s `shouldKeepExistingConnection`. Four modules take
divergence rows and one takes a **deliberate-deferral** row; see the six
new entries in the verdict table below. Three things this ADR had not had
to say before. **First**, the module-evaluation-order constraint recorded
at #500/#501/#538 has a *second, independent* form that no earlier row
names: `fixtures/demo-task-state.ts` must stay provably side-effect-free
at top level or Rollup retains it and `pnpm assert-no-fixtures` fails —
that file's own header records a 5.3 KB production-bundle regression from
exactly this — so a seam call inside a top-level `const` defeats
**tree-shaking**, not merely the readiness guard. `TRIPS_CALENDAR_BINDING_KEY`
is read at `demo-task-state.ts:597` in a top-level `const boundTripsBinding`
and therefore stays literal TS for both reasons at once. **Second**, this
slice establishes *"sink only what has a demonstrated second caller"* as
the tie-breaker the ADR's own reversibility asymmetry implies: a divergence
row is a paragraph the next slice can amend, while a sink is a wasm
surface, a UniFFI surface, generated Kotlin and a web rewire, unwound only
by amendment. `msUntilRotation` and `tripsCalendarId` were both proposed
for sinking and both withdrawn under it — the first because Play Services
re-authorizes silently and so has no Android caller for a proactive
re-mint schedule (GIS's missing refresh token is what makes it necessary
on the web, and that is a web fact), the second because its live consumer
is `screens/vacation-pane/vacation.ts` and the vacation pane's sink is
#534's, which must decide it with its real callers in hand. **Third**, the
one-door mobile seam has a *lock* consequence: `MobileTaskHost` guards its
state with a single `tokio::sync::Mutex` held across the sync cycle's
awaits, so the calendar half takes its own mutex inside the same object —
sharing it would let a slow calendar fetch stall a sync and vice versa.
The compensating gain is real and is why one door was chosen over
mirroring `ffi-web`'s two: the core holds *both* the synced binding and
the per-device selection, so `effectiveSelection` runs entirely inside
Rust and Kotlin never carries the fact across a seam the way `App.tsx`
must.
**Context:** the Android-client grilling of 2026-08-14, opened on
[#141](https://github.com/JddAndrewLauren/hummingbird/issues/141) when the
build went from planned to started — core maturity (the #95/#114 stack) is
closed, and map #130's sequence puts the native client next. The decision
was reached through a written tradeoff analysis in that session and
operator-confirmed. Amends
[ADR-0015](0015-the-standing-question-read-contract.md): its Rust/TS
carve-out — "bespoke answer and band live in TS `screens/*.ts` pure
modules" — was drawn in a one-client world, and this ADR redraws the line
for the multi-client one.

## The decision

**Decision logic sinks into `hummingbird-core`. Rendering stays in each
client, written natively.**

*Decision logic* is anything two clients must answer identically or one of
them is wrong: orderings (frontier, triage, done, ledger), urgency and
priority, form validation rules, and the standing-question band functions
**with their thresholds**. *Rendering* is everything whose wrongness is a
matter of taste rather than correctness: collapsed headlines, icons,
layout, and per-surface presentation preferences.

The core's consumers reach the sunk logic through the seams that already
exist: the web client through `client/ffi-web` (wasm-bindgen), the native
clients through `client/ffi-mobile` (UniFFI, which generates Kotlin for
Android/Wear and Swift for iPad from the same crate). No decision function
is ever hand-copied into a client language.

## Sequencing: sink-as-you-go, per screen, as a mandatory step

The sink is **not** a standalone refactor and **must not** become one.
Building an Android screen has a fixed first step: sink that screen's
decision modules into `client/core`, rewire the web client to consume them
through the wasm seam, *then* build the native screen against the same
core. The forcing function is the build order itself — a screen cannot
land with a Kotlin copy of a decision function, so the sink can neither
run ahead of the app (a blocking big-bang) nor silently lag it (the
"opportunistic" failure mode where the migration never happens).

## Why the carve-out moves

ADR-0015 put the bespoke answer/band logic in TS to protect the visual
iteration loop, and sank the two things that "must not drift" — the
freshness clamp and alert liveness — into Rust. That *mechanism* is
exactly right and is what this ADR extends; only its *inventory* was
one-client-shaped. The moment a second client renders the same band, the
band function itself is a thing that must not drift: a phone and a browser
disagreeing about "Trash Tonight" is precisely the failure the carve-out
exists to prevent.

The facts that decided it:

- The screen-logic layer is ~7.3k lines across 31 modules, and the load-
  bearing ones are near-pure functions over wire DTOs — `urgency.ts` and
  `priority.ts` import nothing at all. This is the cheapest possible code
  to sink and the most expensive to hand-copy.
- Three-plus clients are planned (web, Android, iPad; Wear shares the
  Kotlin side). `ffi-mobile` already declares Kotlin *and* Swift targets:
  one sunk module reaches every client through generated bindings.
- Map #35's locked decision list already says **maximum shared core**, and
  the repo-wide rule for the worker build says keep every decidable thing
  in a natively-tested lib. This ADR makes the client layer obey both.

## What ADR-0015 said that stays true

**Thresholds stay beside their band functions.** ADR-0015's reasoning —
the driver is the cost of a wrong answer, per question, not a universal
multiplier of cadence — is untouched. The band function and its threshold
move *together* into the core; they are decision logic of the same kind.

**The visual iteration loop stays protected**, now scoped to what it
actually covers: rendering. Headlines, icons and layout remain client
code, editable without recompiling wasm or regenerating bindings.

**The freshness clamp and alert liveness stay where ADR-0015 put them** —
this ADR adds to that set rather than moving it.

## Rejected alternatives

**A per-client reimplementation with translated tests as the drift gate.**
Two copies at Android, three at iPad, each with its own translated test
suite. The gate catches drift only where someone thought to translate a
test, and every new decision is written N times forever. This contradicts
"maximum shared core" as a steady state.

**Sinking everything first, before the app.** A ~7.3k-line blocking
refactor between the operator and the first Android screen, and it drags
genuinely view-shaped code (headline strings) into Rust where the
iteration-loop argument still wins.

**A worker round trip for the web's half.** Rejected at M1-1 (below): the
modules being sunk run synchronously during React render, and a
`postMessage` hop cannot be spliced into a render.

**The named risk, and its exit.** The seam cost is not zero: each sunk
module is a wasm/UniFFI surface change. If the first screen's sink shows
binding churn dominating — costing more per module than a port would —
the fallback recorded at decision time is to keep only the clearly-shared
decisions (orderings, urgency, priority, bands) in core and let
view-adjacent modules go per-client. That fallback is an amendment to this
ADR, not a silent drift.

## The web seam, fixed by M1-1's probe

*Amended 2026-08-15 ([#499](https://github.com/JddAndrewLauren/hummingbird/issues/499)):
this ADR was accepted with the web-side mechanism unresolved. "Rewire the
web through `ffi-web`" was not a like-for-like swap, because the wasm core
is instantiated only inside the SharedWorker while every module being sunk
runs synchronously during React render. M1-1 was run as a probe to fix it,
and this section is its outcome.*

**The mechanism.** A **second instantiation of the same
`hummingbird_ffi_web` module, on the main thread**, exposing free
`#[wasm_bindgen]` functions over scalars and JSON
(`client/ffi-web/src/decisions.rs`), wrapped by synchronous TypeScript in
`client/web/src/decisions/seam.ts` and awaited in `main.tsx` before the
first `createRoot().render()`. Every later M1 issue rewires through that
one file; no module imports the generated package directly.

**Why not a worker round trip.** Per-keystroke capture validation and
per-render ordering/faceting cannot absorb an async hop, and the grouping
axis and facet selection are main-thread UI state the worker cannot see.

**Scope note: this does not violate ADR-0010.** Checked against that ADR's
text rather than its title. Its three failure modes — divergent mirrors,
two sync timers, duplicate writes — each require a second *queue*. The
functions reached through this seam construct no `Core`, open no storage,
start no timer, and take `now` as an argument. The rule is structural, not
a promise: nothing under `client/web/src/decisions/` may enter
`core.worker.ts`'s static import graph, and
`client/web/src/worker/worker-import-graph.test.ts` fails the build if it
does. A function that needs a core, storage or a clock belongs behind the
SharedWorker's existing request protocol instead.

**What the probe measured** (Chromium via Playwright over the production
build, and node; full numbers in #499's PR). Instantiation added to the
loading gate: **p50 9.1 ms** (budget: 300 ms). One 100-item
`TaskItemDTO` payload crossing as JSON, `JSON.stringify` included: **p50
0.1 ms, first call 1.9 ms** (budget: single-digit ms). Added bytes:
**+411 bytes of wasm** for the first sunk decision, plus a one-time
**37.8 KiB raw / 8.1 KiB gzip** duplicate of the wasm-bindgen JS glue,
which is the seam's whole fixed cost and is not paid again per module.
vitest instantiates the module in both `node` and `jsdom` from one
`setupFiles` entry, with no per-file hack. No flip condition was hit.

### What does *not* sink in M1

Later M1 briefs quote their row. A verdict of "stays" here is scoped to M1,
not permanent — it is where the line fell for the capture/Now slice.

*Amended 2026-08-21 ([#564](https://github.com/JddAndrewLauren/hummingbird/issues/564)):
**four of the calendar rows below were reasoned against a mechanism Android
no longer runs, and one more row is added.** #564 was scoped on 2026-08-18
around a native `AuthorizationClient` grant; the operator's 2026-08-21
decision replaced it with ADR-0028's authority-minted route, on the grounds
that two mechanisms for one operator's one calendar is a maintenance tax
with no matching risk reduction. The corrections, each named by the row it
corrects:*

- *`msUntilRotation` — **now sunk in spirit, ported in fact, and it does
  have an Android caller.** The row's argument was that Android
  re-authorizes on demand and keeps no rotation schedule; under ADR-0028 the
  phone holds a ~1-hour Google access token exactly as a browser does and
  must rotate ahead of it. The 5-minute margin is ported into
  `ffi-mobile/src/calendar_token.rs` rather than sunk, for a reason the
  original row would have accepted: what it is coupled to is the
  **authority's** `CACHE_REMINT_MARGIN_MS`, which is not a client decision
  at all, and both copies pin themselves against that constant's own source
  text. The "exactly one caller" clause is what is stale, not the verdict.*
- *`shouldKeepExistingConnection` — verdict unchanged (sunk in effect on
  both clients), reasoning corrected: the Android case is not "cannot reach
  Play Services to confirm a grant it still holds" but "cannot reach the
  **authority** to mint". Same rule, same cost of getting it wrong, a
  different unreachable party.*
- *`persistence.ts` — verdict unchanged (storage mechanism), corollary
  corrected: the phone still persists strictly less than the browser, and
  still never a credential, but the reason is that the access token is
  **held in Rust for its lifetime and re-minted at the authority**, not that
  Play Services re-mints it silently. `CalendarPrefs.kt` carries a source
  gate against anything token-shaped appearing in it.*
- *`remint-health.ts` — **the deferral survives, for a different reason.**
  Not "Android's `AuthorizationClient` failure vocabulary is unseen"; the
  phone's vocabulary is the authority's, and it is the same seven codes the
  web already has. What defers it is that #419 Phase 9's degradation is an
  ITP-specific remedy with no Android counterpart, so the *question* is
  shared while the *remedy* is not. #564 answers the shared half in
  `calendar_token::connection_state`, which decides which of four
  Source-connection states each code puts the device in — that function is
  the phone's `remint-health.ts`, and a later slice may sink the pair.*
- *`connect-error.ts` — verdict unchanged (display copy), with the note that
  four of its nine arms are now unreachable on **both** clients, not just on
  Android: `gis.ts` is deleted, so `popup_failed_to_open`, `popup_closed`,
  `gis_script_load_failed` and `gis_unavailable` can no longer occur
  anywhere. That is #577's residue, not this ADR's, and it is a cleanup for
  whichever slice next touches that file.*

*A second correction, recorded 2026-08-21 once #564/#621 were built: **three
rows above describe a sink that did not land the way they say.** These are
not reasoning errors like the four above — they are rows that ran ahead of
the code, which is the exact drift this ADR's own sequencing rule exists to
prevent, so they are rewritten rather than annotated:*

- *`selection.ts`'s `effectiveSelection`/... — **partly sunk, and the web is
  not rewired.** `effectiveSelection` itself is now
  `calendar::selection::effective_selection` in `hummingbird-core`, and the
  mobile seam derives the polled set through it (the picker's list ∪ the
  bound Trips calendar, at the long horizon) on every push and every tick.
  What did **not** happen is the "web rewired in the same slice" half:
  reaching core from a browser needs an `ffi-web` export #564 did not scope,
  so `client/web/src/calendar/selection.ts` still runs its own copy behind
  `useCalendarWiring.ts`. `acceptSelectionChange`, `unavailableSelectedIds`
  and `toggleCalendarId` are **not** sunk at all: they are picker
  affordances, and Android's picker has no locked Trips row to refuse yet —
  a real gap, and the one behaviour where the two clients still differ.*
- *`connection.ts`'s `shouldKeepExistingConnection` — "sunk in effect on
  both clients" was the hedge doing the work. It is **ported, not sunk**:
  `connect_calendar`'s `was_opted_in || minted` and the TS predicate are two
  spellings of one rule. They agree on all four inputs today and nothing
  pins them to each other. Same verdict as `msUntilRotation`, and it should
  have been recorded the same way.*
- *`calendarRequests` (the row further down, "calendar-request building stays
  per-client") — **reversed for the computation.** Both windows are now
  built core-side (`weekend_calendar_interval`, `vacation_calendar_interval`
  in `decisions::panes`), because Android needs the identical bounds and the
  7/730-day horizon. The row's tzdb argument is answered by
  `vacation_zone_queries`, which pre-resolves the horizon's two civil ends so
  the core **can** decide the interval in advance of asking. The web's copy
  is likewise un-rewired pending the same `ffi-web` export, and diverges on
  one path: an unresolvable zone makes the web fall back to a UTC slice while
  the core returns `None`.*

*And one row is added, from #564's own work — the first row in this table
to sink something that was left unsunk by an **earlier** ruling and reversed
on the tie-breaker rather than on a new argument:*

| Module | Verdict |
|---|---|
| `weekend.ts`'s `mergeWindow` per-day entries (sunk at #564/#621) | sunk to `decisions::panes::weekend`'s `merge_window`/`count_kinds` — **reversing #534's own verdict**, which folded the merge straight to a `WindowCounts` tally on the grounds that no decision read an entry's `id`/`title`/`at_ms`/`anchor`. That was right with one client. Android's expanded weekend card is the second caller ADR-0025's tie-breaker asks for, and what would have been duplicated is #122's due-beats-scheduled dedupe and both its residues (`also_scheduled_on`, `deadline_outside_window`) — a rule two hand-written merges would each have to get right, where the one that got it wrong would look entirely plausible. The counts are now tallied **from** the entries, so they can no longer disagree with the list they describe. What still does not cross is the DTOs: an entry carries a `source_id` and each host re-attaches its own event or item |


| Module | Verdict |
|---|---|
| `size-energy.ts` | rendering; re-imports vocabulary via the shim |
| `blocked-reason.ts` | rendering |
| `frontier-prefs.ts` | view prefs |
| `capture-dictation.ts` | DOM caret, ADR-0022 |
| capture-meta's form-adapter half | slider indices / `""` sentinels |
| `capture-destination.ts` | type-only |
| `FrontierColumns.tsx`'s `URGENCY_EDGE`/`LABEL` maps | presentation of the band, not the band |
| `field-vocabulary.ts`'s `SIZE_OPTIONS`/`ENERGY_OPTIONS`/`CONTEXTS` (added at #500) | stays a literal TS array, module-evaluation-order constraint (see the amendment above); pinned against `hummingbird_core::decisions::vocabulary` by a test, not sunk at runtime |
| `frontier-facets.ts`'s `SIZES`/`ENERGIES`/`FACETS` (sunk at #501) | the rule sank (`decisions::frontier`'s `Facet`, `NO_CONTEXT`, `matches_facets`, `apply_facets`); the three arrays stay literal in `seam.ts` for the same module-evaluation-order reason as `field-vocabulary.ts`'s, pinned against the core by `seam.test.ts` rather than sunk at runtime — no longer the surviving unpinned copy the #500 review flagged |
| `frontier-columns.ts`'s `FRONTIER_AXES`/`DEFAULT_FRONTIER_AXIS` (sunk at #501) | the grouping rule sank (`decisions::frontier`'s `FrontierAxis`, `group_frontier`); the two constants stay literal in `seam.ts`, same reason and same pinning pattern |
| `priority.ts`'s `priorityRank` (sunk at #501) | the ordering rule sank (`decisions::frontier`'s `priority_rank`, canonical); the TS function stays literal — `PRIORITY_OPTIONS` reads it at module-evaluation time, same constraint as `field-vocabulary.ts`'s arrays — pinned against the core by `seam.test.ts` (`priorityRankFromCore`) rather than sunk at runtime |
| `rules/backtest.ts:52`, `rules/deadline-picker.ts:32` | known drift — local re-derivations of the deadline reading, out of M1's rewire. **Sunk at M4 (#540)**: both are now `hummingbird_core::decisions::rules::{backtest,deadline}`, reading `hummingbird_domain::deadline`'s `deadline_sort_key`/`shift`/`minutes_until`/`parse_duration` |
| `rules/operators.ts`'s `legalOperators`/`defaultOperatorFor` (sunk at #540) | sunk to `decisions::rules::operators`, which derives every answer from `hummingbird_rules_engine::Operator::is_legal_for` rather than the hand-maintained twin table this module asked to be "kept byte-identical" with nothing mechanical connecting them |
| `rules/duration.ts` (sunk at #540) | sunk to `decisions::rules::duration` — the ADR-0013 duration grammar and the #138 alarm-interval warning, parsing through `hummingbird_domain::parse_duration` instead of a second regex and a second unit table |
| `rules/validity.ts` and `registry.ts`'s `fieldsForKind`/`fieldType` (sunk at #540) | sunk to `decisions::rules::validity`. Takes the registry as an **argument** rather than reading `hummingbird_domain::EVENT_KINDS` directly: the catalogue a client edits against is the one its authority exported, not the one its binary compiled — an invalid-rule badge is a trust signal and must answer against what the reader was shown |
| `rules/condition-editor.ts` (`widgetFor`, `newCondition`, `retypeCondition`, `toggleNegate`) (sunk at #540) | sunk to `decisions::rules::editor` — not one of #540's five named items, and forced by them: the phone's create-and-edit form needs the same kind → field → operator → widget cascade, and this ADR forbids Kotlin holding a per-row decision function, so a Kotlin copy would have been the third |
| `rules/operators.ts`'s `OPERATOR_LABELS`, `registry.ts`'s `kindLabel`/`kindOptions` (#540) | **stays** — display copy plus the registry's own declared order. Two clients wording "is within the next" or "Calendar event" differently is a difference, not a bug |
| The epoch ⇄ civil wall-clock conversion, at each host's edge (#540) | **stays per-client, deliberately.** `client/core` holds no tzdb (its `Cargo.toml` argues this at length), so every rules function takes an already-resolved deadline-shaped `now`, exactly as `decisions::urgency` does. The backtest needs *two* readings of one instant — `occurred_at` is stamped UTC by the authority, `deadline`/`scheduled_date` are device-local civil strings — so `BacktestClock` names both frames at the boundary rather than letting the core infer either |
| Calendar / #169's two doors | out of M1 entirely. **Ruled at M4 (#564)** for `client/web/src/calendar/`'s seven modules — the six rows below, plus the two withdrawals recorded in the amendment above |
| `selection.ts`'s `effectiveSelection`/`effectiveCalendarIds`/`acceptSelectionChange`/`unavailableSelectedIds`/`toggleCalendarId` (sunk at #564) | sunk. Not named by #564's own body, and the strongest candidate in that directory: these derive **which calendars a device polls** from two facts held in different places — the per-device selection and the *synced* `trips-calendar` binding — so two clients disagreeing means one polls the wrong set off the same `settings` row. `unavailableSelectedIds` carries the same false-quiet distinction ADR-0015 rules out elsewhere ("we haven't looked" must not read as "none of these exist"). The core already owns the key vocabulary (`bindings::BindingKey::TripsCalendar`), so the sink invents nothing |
| `connection.ts`'s `shouldKeepExistingConnection` (sunk at #564) | sunk. "May a failed interactive attempt un-connect this device?" — answered `no`, and its own header records the cost of `yes`: losing the last-good snapshot and the Reconnect affordance together, so one cancelled consent would cost the reader their offline context. On Android the same rule is what keeps an **offline** phone from rendering as a never-connected one, since the phone cannot reach Play Services to confirm a grant it still holds |
| `connection.ts`'s `msUntilRotation` (#564) | **stays** — and the reason is the tie-breaker, not the shape. It exists because GIS issues browser SPAs no refresh token, so the web must schedule a proactive re-mint against `expiresAtMs`. `AuthorizationClient` re-authorizes silently in later sessions while the grant stands, so Android calls `authorize()` on demand and has no rotation schedule to keep. Sinking it would cross the seam for exactly one caller |
| `connect-error.ts` (#564) | **stays** — display copy, on the same verdict as `OPERATOR_LABELS`/`kindLabel` and `wasteGapReason` ("the kinds sank, the words did not"). Four of its nine arms name browser popup and GIS-script failures (`popup_failed_to_open`, `popup_closed`, `gis_script_load_failed`, `gis_unavailable`) that cannot occur on Android, so a sink would carry permanently unreachable branches across the seam with no Kotlin caller able to reach them and no test able to justify them |
| `connect-pending.ts` (#564) | **stays** — `pageshow`/`visibilitychange`/bfcache wiring, a platform fact on #538's line-splitting verdict. Android has neither bfcache nor the redirect flow that arms the flag, so there is no second answer to disagree with |
| `redirect-return.ts` (#564) | **stays** — the redirect flow is web-only. Note the shape it lands in, which is the one this ADR wants: it *consumes* a sunk rule (`shouldKeepExistingConnection`) while staying per-client itself |
| `persistence.ts` (#564) | **stays** — storage mechanism (`localStorage` vs Preferences DataStore). What must not diverge is the *policy*: never the credential, only the opt-in flag and the selection. That is stated in the module headers and here, not expressed as a sunk function. #564 adds an Android-specific corollary — the Google access token is **never** persisted at all there, since Play Services re-mints it silently, so the phone stores strictly less than the browser does |
| `remint-health.ts` (#564) | **deliberate deferral, not a divergence.** Its *question* — "does this error mean a human must be involved?" — is decision logic Android must also answer, but over an `AuthorizationClient` outcome vocabulary no one has yet seen on hardware, and #419 Phase 9's degradation is an ITP-specific remedy that must not be ported blindly. Deferred until Android's real failure codes are in hand; the slice that sees them decides it. Recorded here so the lag is a decision rather than the "silently lag" drift this ADR's sequencing rule exists to prevent |
| `TRIPS_CALENDAR_BINDING_KEY` (#564) | stays literal TS, pinned by `seam.test.ts`, for **two** constraints at once — the familiar "used before ready" throw, and a second one no earlier row names: `fixtures/demo-task-state.ts:597` builds `const boundTripsBinding` at top level, and that module must stay provably side-effect-free or Rollup retains it and `pnpm assert-no-fixtures` fails (its header records the 5.3 KB regression). A seam call there defeats **tree-shaking**, independently of readiness |
| `selection.ts`'s `tripsCalendarId` (#564) | **not sunk here — handed to #534.** Its live consumer is `screens/vacation-pane/vacation.ts`, so it belongs to the vacation pane's sink, which #534 decides with the seven pane examples in hand. Taking it at #564 would land a pane-lane decision while the pane lane's own gate (#533's zone-bridge ergonomics checkpoint) is still shut |
| the NDJSON **line splitting** (`skills/ndjson.ts`'s `takeLines`, okio's `readUtf8Line`) (M4, #538) | stays per-client: a byte-level stream reader is a platform fact, not a decision. The web decodes a `ReadableStream` with a streaming `TextDecoder`; Android reads okio's buffered source. What each *line means* did sink (`decisions::skills::envelope`) |
| the **transport** — `run-skill.ts`, `route-run.ts`, `skills/SkillRunner.kt` (M4, #538) | stays per-client: `fetch` + `AbortSignal` vs OkHttp + `Call.cancel()` have no shared expression, and neither decides anything. Each reports what happened to *its* socket (no token / never resolved / a status / the stream ended) and the core answers with the next state |
| `skills/decline.ts`'s `NO_TOKEN`/`NO_TERMINAL_LINE` and `grill-turn-state.ts`'s `OUTSIDE_SCHEMA` (M4, #538) | the rule sank (`decisions::skills::decline`, `grill::OUTSIDE_SCHEMA`); the three constants stay literal TS for the same module-evaluation-order constraint as `field-vocabulary.ts`'s arrays — `route-run.ts`/`useMicrotaskWiring.ts`/`useGrillWiring.ts` read them at module evaluation, statically reachable from `main.tsx` — pinned against the core by `seam.test.ts`. Kotlin needs no equivalent: it never holds the strings at all |
| #274's `backend-registry.ts`/`backend-selection.ts`/`route-plan.ts`/`reachability-memo.ts`, and `microtask-affordance.ts` (M4, #538) | out of #538 deliberately — the probe needed one lane end to end, not the whole surface. **Decided at #539** (see the amendment above), in the four rows below |
| `microtask-affordance.ts`'s `microtaskAffordance`, `grill-review.ts`'s `wouldStrandPlan`/`demotesFromFrontier`/`planReplacementLabel` (M4, #539) | sunk to `decisions::skills::{affordance,review}` — the phone's item detail and review card ask the same questions of the same steps, and a Kotlin copy would have been the third. `microtask-affordance.ts` is now a seam wrapper; its `liveUndoneSteps` had no caller left after the sink and was deleted rather than kept |
| `backend-selection.ts`'s `readBackendSelection`, `backend-registry.ts`'s `fallbackEntry` (M4, #539) | the *rules* sank (`decisions::skills::backend`'s `resolve_backend_selection`, `fallback_backend_id`, `declined_backend_fallback`) — which id a stale selection degrades to, and which one a declined pin offers next |
| `backend-registry.ts`'s `BACKEND_REGISTRY` entries and Android's `BackendPreference.ENTRIES` (M4, #539) | **stays per-client**: label, model, endpoint and timeout are configuration, not a decision two clients could disagree about being *wrong*. Both rules above take the registry as a bare ordered list of ids — the only part of an entry either reads |
| `route-plan.ts`/`reachability-memo.ts` (M4, #539) | **stays per-client, for now**: Android runs one backend at a time under the device preference and plans no Auto sequence, so there is a single live caller. Sinking a router from one arm is the trap #533 named and #534 honoured — #275/#276 (on-device and home runners) are what would give it a second |
| `item-actions.ts`'s `applyItemAction`/`resolveFallbackPending` (M1-4, #502) | screen-local optimistic UI reconciliation over `TaskItemDTO` — `Date.now()`, `archivedAt` writes and the live-vs-optimistic `pending` merge are not a decision two clients could disagree about, even though the same file's affordance rules (`availableActions`, `canMarkDone`, `canGrill`, `grillButtonLabel`, `applyItemAction`'s stage lookup) did sink |
| `shell/sync-outcome-informative.ts`'s `isInformativeSyncOutcome` (M4, #535) | stays literal TS, for a **new** reason distinct from the module-evaluation-order rows above: `worker/ports.ts` needs this exact predicate and runs inside `core.worker.ts`'s own static import graph (ADR-0010), which must never statically reach the main-thread seam (`worker-import-graph.test.ts`'s gate) — a static import anywhere in a file pulls that file's whole graph in regardless of which export is used, so the predicate could not share a file with `sync-status.ts`'s seam-backed functions. Pinned against `decisions::settings::is_informative_sync_outcome` by `sync-outcome-informative.test.ts` |
| `theme/ThemePreference.kt`'s `resolveDarkTheme` (M4, #535) | stays per-client, on `frontier-prefs.ts`'s existing "view prefs" verdict widened to cover a theme choice explicitly — a device's dark/light/system preference is not a decision two clients could disagree about |


## The zone bridge, fixed by M4's probe

*Amended 2026-08-18 ([#533](https://github.com/JddAndrewLauren/hummingbird/issues/533)):
the standing-question panes were the last big body of web-only decision
logic, and the one obstacle no earlier M1 slice hit. This section is the
probe's outcome.*

**The obstacle.** A pane is **civil-date reasoning**. A bin collection
happens on a day at an address, not at an instant on a device; "tonight"
must flip at the address's midnight, not the reader's. So every pane rule
that touches a day needs `(zone, instant) -> civil date` and
`(zone, civil date) -> instant`, and `hummingbird-core` has no zone table
to answer either with — deliberately, and at a measured price
(`client/core/Cargo.toml`: the table took the release wasm from 525 KB to
1.41 MB, brotli 175 KB -> 247 KB over the wire).

**The mechanism.** A **two-phase crossing**
(`client/core/src/decisions/panes/zone.rs`). Phase one: the core names
every fact it needs as a `ZoneQuery`, each carrying a deterministic
`key()`. Phase two: the host resolves what it can — `Intl.DateTimeFormat`
on the web (`client/web/src/screens/questions/zone-bridge.ts`, over the
existing `waste-pane/zoned-day.ts`), `java.time.ZoneId` on Android — and
hands back a `key -> fact` table the core ranks against.

**A zone the host cannot resolve crosses back absent.** Not a null, not an
empty string, not a `known: false` flag: the host simply omits that key.
Every reader goes through `ZoneFacts::civil_date`/`midnight_ms`, which
answer `None`, and the rule turning `None` into a gap
(`WasteGap::UnresolvableZone`) is a core decision like every other one.
There is no accessor that fabricates a fallback, which is what stops a call
site inventing one. The host contributes a lookup and no judgement.

**What is not a zone question.** Calendar arithmetic over `YYYY-MM-DD`
strings needs no tzdb, so it is Rust: `is_civil_date`,
`civil_days_between`, `add_civil_days`, `weekday_index`. That mirrors
`zoned-day.ts`'s calendar half while its `Intl` half stays TS — and
`weekday_index` is deliberately an *index*, `0` = Sunday: which day it is,
is a decision; what that day is called is a rendering.

**The cross-host pin.** `client/core/tests/fixtures/panes/*.json` is read by
both suites (`client/core/tests/pane_fixtures.rs` and
`client/web/src/screens/questions/shared-fixtures.test.ts`), on the
`race.test.ts` precedent. Each scenario carries the resolved `zoneFacts`
table, which is what makes a tzdb-free Rust suite possible at all; the TS
side additionally asserts **its own `Intl` resolver reproduces that table
exactly** before running a scenario. That assertion is the actual cross-host
pin, and it is what a `java.time` resolver is held to at #536.

**The ergonomics verdict, and the flip condition.** Two phases were
tolerable: the core-side pair (`zone_queries` / `rank_panes`) is one extra
function per surface, the web-side resolver is a 12-line module with no
opinion in it, and the whole crossing is JSON over the existing stateless
seam — no new mechanism, no state, no ADR-0010 exposure. The flip condition
stands and is unchanged: **if the two-phase shape proves intolerable, the
answer is a tz library in the core via a further amendment to this ADR**,
not a per-client re-derivation of the day.

**Scope of this slice.** Waste alone. `zone_queries`/`rank_panes` ship with
a one-question list (`decisions::panes::SUNK`) so #534 grows a list rather
than an API, and the web deliberately keeps ranking per-question — hoisting
onto the batched path would change `contract.ts`'s `QuestionDef`,
`registry.ts`, `RankedRegion.tsx` and all seven unsunk panes, which is a
rewrite rather than a probe.

### What does *not* sink with the panes (#533)

| Module | Verdict |
|---|---|
| `waste.ts`'s `wasteHeadline`/`wasteCollapsedHeadline` | headline wording; a second client phrasing "Trash tonight" differently is a design choice, not a bug |
| `waste.ts`'s `wasteGapReason` | the one place a `WasteGap` **kind** becomes a sentence — the kinds sank, the words did not |
| `waste.ts`'s `BIN`/`wasteGlyphs`, `contract.ts`'s `PaneGlyph`/`MAX_GLYPHS`/`boundedGlyphs` | glyphs and their colours are rendering; the cap is the shell's own furniture rule |
| `WastePaneExpanded.tsx` and every other `*-pane/` `Expanded` | whole renderings |
| `contract.ts`'s `QuestionDef`, `registry.ts` | the web's own wiring shape (a React `ComponentType` cannot cross) |
| `collapse.ts`, `aside-prefs.ts`, `RankedRegion`'s sampling state | device-local view state, on `frontier-prefs.ts`'s existing verdict |
| `zoned-day.ts`'s `Intl` half | **becomes the web's zone resolver** for the bridge rather than a pane rule — unchanged file, its own suite passes as-is |
| `contract.ts`'s `BAND_ORDER`/`QUESTION_ORDER` | the vocabularies sank (`decisions::panes::contract`); the two arrays stay literal TS for a *harder* version of #500's module-evaluation-order constraint — `registry.ts` builds `QUESTIONS` at module evaluation and reads `QUESTION_ORDER` there — pinned against the core by `seam.test.ts` |
| `waste.ts`'s `SOURCE`/`SNAPSHOT_KEY`/`BINDING_KEY`/`STALE_AFTER_MS`/`STREAM_ORDER` | same constraint via `question.ts`'s `sources: [SOURCE]`; pinned by `seam.test.ts`, not sunk at runtime |
| The weekday *word* | per-client, from `WasteFacts.weekdayIndex` — the core decides which day, the client names it |

## What does not sink with the panes, continued (#534)

*The remaining seven panes, sunk at #534, on the same split waste drew at
#533: a question's answer state, band, `withinBand` and its structured
**facts**/**gap kinds** are decisions and sink; its headline wording,
glyphs, and whole `Expanded` rendering are per-client and do not.*

| Module | Verdict |
|---|---|
| `kimi.ts`'s `formatUsd`/`kimiCollapsedHeadline`/`kimiGlyph`/`kimiGapReason` | headline wording, formatting and glyph — `KimiGap` sank as a kind, the sentences did not |
| `github.ts`'s `githubCollapsedHeadline`/`githubGlyph`/`ageWords`/`githubGapReason` | same split; `githubBand`'s stale-poller escalation is decided in the core (`github_answer`), the web only recomputes the *raw* band locally to tell "genuinely imminent" apart from "escalated because stale" when composing the headline |
| `uptime.ts`'s `uptimeCollapsedHeadline`/`uptimeGlyph`/`ageWords`/`uptimeGapReason` | same split as github's |
| `reachability.ts`'s headline sentence (`"Synced"`/`"Last synced" ${relativeAge(...)}`) | `relativeAge`'s wording stays per-client; `reachability_facts`'s `latestAttemptLanded` boolean is what the core decides, so the web is choosing a verb, not deciding one |
| `race.ts`'s `countdown`'s numeric split, `abbreviate`, `seriesLabel`, `raceHeadlineParts`, `raceCollapsedHeadline` | number/name formatting and headline composition |
| `race.ts`'s `dayLabel`/`clock` | explicitly device-local wall-clock words (ADR-0015) — no zone anywhere, deliberately |
| `race.ts`'s `RaceView.liveAlert`'s own `title`/`body` | the alert join stays client-side (`liveAlertFor`, re-derived from `inputs.paneReads` directly) — `race.rs`'s `RaceFacts.hasLiveAlert` is the boolean the *band* reads; the alert's own display fields are data the client already holds locally and would be a whole-DTO re-crossing for no decision to read |
| `weekend.ts`'s `weekendWindow`/`weekendBand`/`weekendWithinBand` | **stays literal TS, pinned rather than called** — `weekend.test.ts` calls `weekendWindow` at `describe`-body top level, before `wasm-setup.ts`'s `beforeAll` resolves `initDecisions()`; a collection-order version of #500's module-evaluation-order constraint. Pinned against `weekend.rs`'s own `weekend_window`/`weekend_band`/`weekend_within_band` by `weekend-window.shared.test.ts`. `weekendAnswer` (called only inside `it()` bodies) does cross |
| `weekend.ts`'s `mergeWindow`/`WindowEntry`/`countKinds`/`timeLabel`/`shortDayLabel`/`dayKeyOf` | the full per-entry merge, with titles, ids and anchors — the decision only ever needs the *counts* (`weekend.rs`'s `WindowCounts`), and every title/id crossing the seam with no decision reading it would violate `inputs.rs`'s own "do not re-cross whole DTOs" discipline |
| `weekend.ts`'s `entryUrgency` | reads `computeUrgency`, already `hummingbird_core::decisions::urgency` (M1-2, #500) under a different name — nothing second to sink |
| `vacation.ts`'s `Trip.name`/`tripName` | `vacation.rs`'s `Trip` carries no `name` field — no core decision reads it (only the headline does), so this file recovers it locally by matching a core `Trip`'s `id` back to the `CalendarEventDTO` it came from |
| `vacation.ts`'s `vacationHeadline`/`tripDateRange`/`tripDayLabel`/`MONTH_NAMES`/`civilParts` | headline and date-range wording |
| `vacation.ts`'s `vacationSetup`'s `Bound` arm's `read` field | the seam carries only `calendarId` (`vacation_setup_kind`'s kind-only projection — `VacationSetup::Bound` itself borrows the inputs' own event slice and has no `Serialize`); `read` is attached locally from the same `calendarReads` the core already consulted to decide `Bound`, never a second guess about its state |
| `weekend-pane/question.ts`'s and `vacation-pane/question.ts`'s `calendarRequests` (`vacationCalendarInterval`, the weekend window's civil bounds) | ~~calendar-request building stays per-client, keeping the tzdb at request-build time~~ — **reversed at #564/#621, see the second amendment above.** Both windows are computed core-side (`weekend_calendar_interval`, `vacation_calendar_interval`); the web's copy stays live and un-rewired until an `ffi-web` export exists. The original argument — that this is a per-render host concern (`useCalendarEventsWiring`'s effect) rather than a fact the core needs decided in advance of asking — is answered by `vacation_zone_queries` pre-resolving the horizon's civil ends |
| `WastePaneExpanded.tsx`-shaped `Expanded` components for all seven (`KimiPaneExpanded`, `GithubPaneExpanded`, `UptimePaneExpanded`, `ReachabilityPaneExpanded`, `RacePaneExpanded`, `WeekendPaneExpanded`, `VacationPaneExpanded`) | whole renderings |
| `kimi.ts`/`github.ts`/`uptime.ts`/`reachability.ts`/`race.ts`/`weekend.ts`/`vacation.ts`'s own `SOURCE`/`SNAPSHOT_KEY`/`BINDING_KEY`/`STALE_AFTER_MS`-shaped constants | same module-evaluation-order constraint as waste's own four (each question's `sources: [SOURCE]` in its `question.ts` is built at module evaluation); pinned against the core's own `*_constants_json()` by `seam.test.ts`, not sunk at runtime |
| The weekday/month *words* everywhere they appear | per-client, from the core's own civil-date/index facts (`weekendDay.date`, `Trip.startDate`) — the core decides which day, the client names it |

## The homework pane's display text crosses, and that is the exception (#675)

*Amended 2026-08-21 (#675).* Every row above that touches an item's own
words sends them the other way: `weekend.ts`'s per-entry merge keeps its
titles, ids and anchors client-side because the decision needs only the
counts, and `race.ts`'s `RaceView.liveAlert` keeps its `title`/`body`
client-side because `hasLiveAlert` is the only thing the band reads. The
rule those two share is `inputs.rs`'s: **display text does not cross the
seam when no decision reads it.**

`homework.rs`'s `HomeworkItem` breaks it. It carries `title`, `deadline`
and `description` out to both clients, and no band, answer state or
`daysAway` reads any of the three — the pane's `Expanded` body is what
renders them. `description` additionally crosses *inward*, as
`PaneItemFacts::description`, which is the one field on that struct no rule
consults.

**This is recorded as a deliberate exception rather than defended as
consistent with the rows above**, because the argument that would make it
consistent does not hold. `HomeworkItem::id` crosses too, so a host could
hydrate the winner's title and notes out of the same `items` list it just
passed in, and the two lookups could not disagree about *which* item won —
the id is the decision. What the crossing buys is narrower than that: one
shape, assembled once, that both clients render without either of them
owning a second "which fields does the body show" list that can drift
between web and Android. That is a real cost avoided and a thin one, so it
was taken with its eyes open.

**It is not a precedent.** A second pane whose subject is the operator's
own items is a decision to take again on its own merits, exactly as
CONTEXT.md says of a second `@homework`-shaped context. The test above is
unchanged for every other pane: if no decision reads it, it stays home.

## The share-payload mapping sinks before its second caller exists (#782)

*Amended 2026-09-03 (#782).*

| Module | Verdict |
|---|---|
| `decisions::share` — `parse_share_payload`, `first_http_url`, `url_host`, `link_display_label` | **sunk on arrival**, with one caller (Android's `ACTION_SEND` alias) and no web share target yet. The tie-breaker is satisfied prospectively: a PWA `share_target` is a known later slice, and the alternative was a Kotlin URL regex in `CaptureActivity` — the kind of hand-copied decision `CaptureFieldSetStructuralTest` already bans in `ui/forms` (date regexes, vocabulary literals) and `ManifestAliasTest` now bans there by name (`Regex(`, `indexOf("http`). What crossed is a mapping (which *field* each piece of a share starts in), a display rule (name → host → URL) and a safety rule (`is_followable_link`: only an `http(s)` URL with a host is drawn as a tap that leaves the app — the web's anchor and Android's `ACTION_VIEW` read the same answer); what did not cross is the seeding itself — `CaptureViewModel.seedFromShare` is a form action, and `Core::capture` still receives the human's title verbatim (ADR-0022) |
| `capture.rs`'s `parse_seam` | untouched. The share mapping seeds a *draft* the human edits; it is not a capture parser, and lives in its own module so that #42's guard stays a named no-op |
| `decisions::share::link_label_problem` — "a name needs a URL" | **sunk** (review finding on #782's own PR). The first cut stated the rule as a form check in five client-side places — `capture-meta.ts`'s `linkProblem`, `CaptureViewModel.canSubmitDraft`, `ItemDetailViewModel.canSave`, and both seams' `capture` guards — each a two-string comparison that looked too small to drift. That is exactly the shape #500 sank `capture_meta_problems` for: the rule and its message now live once, both seams export it (`link_label_problem`/`linkLabelProblem`), the web's capture box and triage/item-panel drafts read it through `seam.ts`, and Android's two ViewModels take it injected beside `metaProblemsFn`, with `CaptureSubmitRefusalTest` gating that production passes the real binding. The authority's own 400 for the same shape stays — it is the wire's rule, and a client cannot be trusted to have asked |
