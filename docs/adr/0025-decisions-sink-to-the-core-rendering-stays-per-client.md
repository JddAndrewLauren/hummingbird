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
| `rules/backtest.ts:52`, `rules/deadline-picker.ts:32` | known drift — local re-derivations of the deadline reading, out of M1's rewire |
| Calendar / #169's two doors | out of M1 entirely |
| `item-actions.ts`'s `applyItemAction`/`resolveFallbackPending` (M1-4, #502) | screen-local optimistic UI reconciliation over `TaskItemDTO` — `Date.now()`, `archivedAt` writes and the live-vs-optimistic `pending` merge are not a decision two clients could disagree about, even though the same file's affordance rules (`availableActions`, `canMarkDone`, `canGrill`, `grillButtonLabel`, `applyItemAction`'s stage lookup) did sink |

