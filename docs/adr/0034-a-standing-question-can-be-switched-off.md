# ADR-0034: A standing question can be switched off, and the Settings roster is what makes that legal

**Status:** accepted · 2026-08-23
**Context:** the fantasy-football grilling of 2026-08-23, opened on a request
for a new standing question (Yahoo fantasy football) that is only in season
17 weeks a year. The question that could not be answered inside the existing
model was not the lane — it was the off switch. Amends
[ADR-0015](0015-the-standing-question-read-contract.md) (a standing question
is *permanently posed*) and
[ADR-0017](0017-the-standing-question-surface-axis.md) (a surface renders
every question declared for it). Terms **Standing question** and
**Salience** are amended in `CONTEXT.md`.

## The decision

**A standing question may be switched off. Off means hidden, silent and
unpolled — one synced fact, read by the surface, the alert lane and the
poller alike. It is legal only because Settings enumerates every question
whether it is on or not.**

## The premise this overturns, and why the overturn is not a violation

ADR-0015 is explicit that a question is permanently posed, and gives a
reason rather than a preference:

> a pane that vanished when nobody had bound it would be a question nobody
> could ever discover

That argument is sound, and it is **load-bearing on an assumption**: that
the *surface* is the only place a question can be discovered. It was true
when it was written — a question existed on Now or on Status, and nowhere
else.

The Settings roster (decision 4 below) falsifies the assumption. Once every
question is enumerated in one place with its state on show, a question that
is off is still discoverable, and the sentence above no longer forbids
anything.

**So the roster is not a companion feature shipped alongside the toggle; it
is the toggle's precondition.** They ship together or not at all. A build
with a toggle and no roster reintroduces exactly the invisible-question
failure ADR-0015 named, and would deserve to be reverted.

`dormant` is untouched and is still the right answer for a question that has
nothing to say. ADR-0015's own example — *"the same race question low again
in the off-season"* — remains a `dormant` pane, not an absent one. Off is a
different fact from dormant: **dormant is a question answering "nothing
now"; off is a question not being asked.** `CONTEXT.md`'s **Salience** entry
carries the distinction.

## The five decisions

### 1. Off means hidden, silent *and* unpolled

Three separable things, and the toggle governs all three:

| Layer | What off does | Why it has to be here |
| --- | --- | --- |
| Surface | the question's panes are not emitted | the visible half of the request |
| Alert lane | nothing is raised | an alert raised server-side cannot be suppressed by a device-local flag |
| Poller | no network call, nothing written | "fantasy is not year-round" is a traffic claim, not a display one |

Rejected: **off = hidden only**, which would leave a device-local view
preference beside the pane collapse state (`bindings.rs`: *"the collapse
state of a pane is not a binding — it is device-local and band-scoped"*).
It fails on the alert lane. An alert that rings while the pane explaining
it is switched off has, under
[ADR-0027](0027-an-alert-opens-the-thing-it-is-about.md), nothing coherent
to open.

Reachable today with no new mechanism: a poller already reads the operator's
`settings` through the ordinary binding route (`race-poll`'s `binding`
module reads which series to poll over `GET /api/settings/race-series`), so
reading an enabled flag and exiting 0 is the same shape.

### 2. The toggle is its own typed fact, not a `BindingKey`

One `settings` row per question, keyed off the **existing
`StandingQuestion`** enum, typed boolean. Absence of a row means
**enabled** — so there is no migration, no backfill, and a question a build
has never heard of is on rather than mysteriously off (the same reading
`Binding::known` gives an unrecognised key).

Rejected: **new `BindingKey` variants** (`fantasy-enabled`, …). Two
reasons. `BindingValue` is `Unset | Text | Other` and `Core::set_binding`
writes only strings, so a boolean routed through that path is the literal
string `"true"` in an editable free-text field — a text box, not a toggle.
And a binding is defined as *"the small cross-device facts a pane needs
before it can answer anything"*; a toggle is not a fact a pane reads, it
decides whether the pane is **asked**. ADR-0015 widened the binding
vocabulary once already, for `homework-link`, with an explicit paragraph
justifying it; a second widening for a third meaning is how a closed
vocabulary stops being one.

Rejected: **one row holding a JSON set of disabled keys**, which has the
tidiest footprint in a table with no DELETE. It loses on CAS. Bindings are
written as entity-level compare-and-swap on one row, so a phone switching
fantasy off and a browser switching race off are two writes to the same row
at the same version — one loses, and the loser is a toggle that visibly
flips back, which is the most corrosive bug a settings screen can have.
Per-question rows make concurrent toggles of *different* questions
structurally non-conflicting. The no-DELETE growth is bounded by the
question vocabulary itself.

### 3. Switching off is prospective, and needs no cascade

A live alert standing when its question is switched off is left alone.
[ADR-0011](0011-context-ingestion-moves-server-side.md) already decided
this class of edit by name: *"Rule edits apply prospectively — from the next
poll onward. A notification about a stale event is noise by definition, so
retroactive firing is a non-feature."*

This is only safe because the sources in question carry `expires_at`, so a
stranded alert settles itself. **A source whose alerts never expire (mail,
per ADR-0014) would strand a live alert forever** under this decision —
`alerts` has no DELETE and ADR-0016's horizon never reaches a live row. Any
future question fed by such a source must answer that before it becomes
switchable.

Rejected: **a dismiss cascade** over the question's live alerts, on #630's
archive-cascade shape. It buys hours of tidiness for a second mutation path
into `alerts` that exists solely for the toggle, and #630 is the cautionary
precedent rather than the encouraging one — it writes `archived_at` onto
live items, there is no item DELETE route, and it is not undoable, which is
why that lane's acceptance is still stalled.

### 4. The question roster sinks into the core; rendering stays per client

One decision module owning, per question: its key, its display label, its
surface, its enabled state, and the `BindingKey`s that answer it — derived
from `StandingQuestion` and `SUNK`, never maintained beside them. The shape
`decisions/roster.rs` took for the bottom nav, under
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md).

The question→binding relation is **new**, and it already exists in the
wrong place: `SettingsScreen.tsx`'s calendar hint reads *"Polled because it
answers **How long to the next vacation**. Change it under Standing
questions"* — the relation `trips-calendar → Vacation` hand-written as
English in one client. Sinking it makes that hint a lookup.

Deriving the roster from `SUNK` rather than from a per-client table is the
part that must not be traded away for speed. If the roster is a TSX table,
an eleventh question means editing `SUNK` *and* remembering a file in
another language, and the failure is a question that polls, rings, and is
invisible in Settings — decision 4's own premise collapsing one slice after
this ADR is accepted.

**Android renders it as a follow-up slice that adds no decisions.** A
deliberate divergence under ADR-0025's carve-out, the same trade the Status
board took (ADR-0033), and entered for the same reason: that surface has no
emulator matrix, so a UI change there owes a device run. In the interval the
flag is still synced — a question switched off in the browser is off on the
phone, which simply cannot change it. That is coherent, and strictly better
than a phone holding its own idea of off.

### 5. The off state is a boolean

Rejected during the session, and recorded because the risk is real and was
**accepted rather than missed**: an off question cannot self-resurrect. A
question switched off in January stays off in September; nothing wakes it,
and a question that is not polling cannot notice that it should be. For the
fantasy lane the concrete cost is missing Week 1 — silently, and it is the
worst week to miss.

The alternative offered and declined was **off-until**: a nullable
timestamp on the same row, evaluated as a pure function of `(stored flag,
now)` exactly as `race-alert-poll` already evaluates its lead time, with an
indefinite off expressed as no date. One field, no new mechanism.

The operator chose the boolean. A future reader tempted to "fix" this should
know it was offered, priced and turned down — and that the fix, if the risk
ever bites, is the wake date and not a cron. A scheduled re-enable is a
second clock owning a cadence the flag already owns, which CLAUDE.md bans.

## Consequences

- Two facts now go stale annually for a seasonal question — its toggle and
  its binding — and neither is self-healing.
- Every question becomes switchable, including the nine that predate this
  ADR. None of them asked for it; the vocabulary is uniform because a
  per-question exception list is a worse thing to maintain than a uniform
  capability.
- `settings` grows one row per question ever switched, permanently. Bounded
  by the question vocabulary, and the reason decision 2 refused an open key
  space.
- Settings becomes load-bearing rather than incidental: it is now the only
  place an off question can be seen. A regression that hides the roster
  hides questions.
