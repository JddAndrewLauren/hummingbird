# ADR-0025: Decisions sink to the core; rendering stays per-client

**Status:** accepted · 2026-08-14
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

**The named risk, and its exit.** The seam cost is not zero: each sunk
module is a wasm/UniFFI surface change. If the first screen's sink shows
binding churn dominating — costing more per module than a port would —
the fallback recorded at decision time is to keep only the clearly-shared
decisions (orderings, urgency, priority, bands) in core and let
view-adjacent modules go per-client. That fallback is an amendment to this
ADR, not a silent drift.
