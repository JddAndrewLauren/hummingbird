# ADR-0015: The standing-question read contract

**Status:** accepted · 2026-08-10 · **amended 2026-08-12 by
[ADR-0017](0017-the-standing-question-surface-axis.md):** the ranked region
is instantiated per surface — `QuestionDef` gains `surface: "now" | "status"`
— so "the ranked region owns Now's Context aside" and "one region, one slot"
are true of Now specifically, not of every surface that will ever exist.
· **amended 2026-08-13 by
[ADR-0021](0021-the-frontier-in-columns.md):** the aside this ADR calls "Now's
Context aside" is renamed **Standing questions** — the `Context` label has been
stale since this ADR replaced the calendar context tile with the ranked region,
and the word is needed for the frontier's grouping axis in the centre column.
· **amended 2026-08-14 by
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md):** the
Rust/TS carve-out is redrawn for the multi-client world — decision logic
(orderings, urgency/priority, band functions with their thresholds) sinks
into the core behind the generated bindings; only rendering stays
per-client. The threshold-beside-band reasoning survives; the pair moves
together. · **amended 2026-08-18 by
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md#the-zone-bridge-fixed-by-m4s-probe):**
**Time** below is written in a one-client world, where the reader's zone
database and the pane's rules share a runtime; a core with no tzdb splits it
into a two-phase answer, and an unresolvable zone becomes a core decision
rather than a host fallback. The invariant — a civil date is never stored as
an instant, and resolves at read time — is untouched.
**Amendments to this ADR follow [the pointer convention](README.md):** what
a later ADR changed is written in *that* ADR, and named here only.
**Context:** the standing-question seam grilling of 2026-08-10, opened on
#118 (settings bindings) and widened when that slice proved undesignable
alone — the bindings exist to pose standing questions, and four throwaway
prototypes had already produced findings that constrain the shared
contract. Discharges #119's finding 3 (the alert↔snapshot join, explicitly
left open by that prototype). Amends
[ADR-0009](0009-the-owned-schema-and-context-lanes.md) (`context_snapshots.payload`
gains a common envelope; `alerts` gains `subject_key`; `SCHEMA_VERSION`
3 → 4), and reverses a position taken earlier in the same session
(that panes span a whole `source`). New glossary term **Salience** lands
in `CONTEXT.md`, and **Standing question** is sharpened there.

The driving requirement, added during the session: **the page must rank the
standing questions.** A race tomorrow high, a vacation 18 months out low,
and the same race question low again in the off-season. Nothing in ADR-0009
or #117 said how, and nothing decides it per-question without inventing a
cross-question comparison somewhere.

## What the prototypes are, and what they were allowed to decide

Four prototypes ran before this ADR, one per question:
`client/web/src/screens/prototype-race-pane/` (#119),
`prototype-weekend-pane/` (#122), `prototype-vacation-pane/` (#121),
`prototype-waste-pane/` (#120), plus a superseded Rust twin at
`server/prototypes/waste-cadence/` whose fan-out models a per-stream world
that does not exist (its lifecycle findings survive; the browser twin
carries them).

*Deleted 2026-08-10 by [#120](https://github.com/JddAndrewLauren/hummingbird/issues/120).
The sentence above is left as written — it is a historical statement about
what ran before this ADR. Where the surviving findings went: `Cadence` and
its arithmetic, the deviation judgement and all three `Deviation` arms are
now `server/city-waste/`, tested and in the workspace CI gates; the
"a daily poll must not restamp `raised_at`, but a correction must" finding
became `AlertIngest::restamp_on_change` on the authority, because a poller
holding an ingest token cannot read the alert back and so cannot decide it;
"stamp with the write clock, never the poll's nominal slot" is that
handler's `now_ms`; "`expires_at` is the end of the later of the two dates"
is ADR-0014's amended `city-waste/v2` entry; and the backward-slide guard
the prototype flagged and left open is `judge::MAX_SLIDE_DAYS`. The
read-side half — `pane`, `Can`, `PaneView` — was already superseded by the
browser twin and is not carried anywhere: the client owns it.*

The rule applied when they disagreed with the plan: **the plan wins on data
and plumbing, the prototypes win on UI.** A prototype was written under
prototype rules — no tests, no error handling, stub mutations — so its
verdict is trustworthy about what a person can read on a screen and about
nothing else. Where a prototype's verdict is overruled below, it is
overruled explicitly and the reason is recorded, because the verdict was
reached by comparison and the comparison is the evidence.

## The seam is the pane's read contract, not the wire payload

Only two of the four questions touch `context_snapshots` at all (races,
waste). The vacation countdown is device-polled with the calendar as
authority; weekend plans is a pure mirror query. What they share is not
where data comes from — it is **the set of states a pane must render** and
**how those states rank against each other**.

So the contract binds the *output* only, plus one shared helper for the
snapshot lane:

```
PaneAnswer = { answerState, band, withinBand, collapsedHeadline, icon? }
```

The expanded form is a bespoke component per question. Nothing generic
could hold a three-can graphic, a session ladder, a day-sectioned weekend
card and a headline that swaps meaning mid-race-weekend.

**`collapsedHeadline` is the one piece of rendered content the contract
carries**, because "smaller type" is a consistency requirement across the
whole collapsed stack, and pane-owned typography would drift most visibly
exactly there.

## Salience

Currency is a **closed band vocabulary**: `live` / `imminent` / `near` /
`distant` / `dormant`. Not a raw score — that manufactures cross-question
comparability nobody can defend — and not time-to-next-moment, because the
weekend pane is a *window* rather than a moment and a session already under
way beats any countdown.

**Every pane owns its own band thresholds.** "45 days to a race" and "45
days to a vacation" need not land in the same band. A live alert may lift a
band, but only through the pane's own judgement — never a generic shell
rule. Placement is the page's; interruption stays the alert lane's.

`dormant` means *posed and answerable, nothing worth attention now*. It is
**not** a synonym for *far away*: the vacation pane's whole winning design
is a tile that sits quietly for 380 of a trip's 395 days and is still worth
reading, so distance alone must never collapse it.

**No human override.** Tripwire for revisiting: two panes tie in a band and
you want one above the other for reasons the band function cannot see — the
fix then is a pin binding, not a finer vocabulary.

**Second tripwire, recorded because it is already visible.** No prototype
needs more than three bands. Waste's winning design is *binary* (`dormant`
vs one loud band; it cannot distinguish a collection four days out from six).
Weekend never reaches `distant` — its window is at most nine days away.
Vacation is the only genuine user of the long tail. The five arms are
therefore justified by **cross-pane ranking alone**, not by any pane's own
expressiveness. If in practice the panes cluster into two or three bands,
the sort collapses onto `withinBand` plus declared order and the band axis
has stopped doing work — that is the signal to cut the vocabulary, not to
extend it.

## Pane = one subject

A question registers once and emits **0..N panes**, one per subject, at
runtime. Races emits one per series named in its binding. In the snapshot
lane a subject is exactly one `context_snapshots` row; in the calendar lane
there is no row and the subject is the question itself.

*Amended 2026-08-21 (#675): a standing question may key on the operator's
own **items** rather than on an outside source.* Every question this ADR
was written against reads a thing the operator does not own — a municipal
feed, a calendar, a race schedule, a CI run, the device itself — and the
weekend pane's items are merged onto days a *calendar* defined, so the
calendar is still its subject. `Homework` is the first whose subject is the
item list itself: it asks "what is the next piece of homework, and what did
I write down about it", and every part of its answer comes from
`QuestionInputs.items`. Three consequences this contract now carries:

1. **A question may have no source and no binding at all.** `Homework`
   declares an empty `sources`, no `calendarRequests` and no binding key —
   it keys on a **hardcoded context literal**
   (`panes::homework::HOMEWORK_CONTEXT`, `@homework`, which
   `decisions::vocabulary::CONTEXTS` pins itself against so the pane and
   the capture forms cannot disagree about the spelling).
2. **`unbound` is then unreachable for it, and that is correct.** Nobody
   binds this question, so there is no setup prompt to route anyone to; an
   empty item list is `answered` + `dormant` ("No open homework"), on
   exactly the `none_in_horizon` reasoning in the table above — a question
   that answered *nothing* is not a question that *failed*. Its only gap is
   the zone bridge's own.
3. **`QuestionInputs.items` is "every live item", never "the items your
   pane should consider".** A question reading it owes its own explicit
   filter. The weekend pane was given one (`panes::weekend`'s
   `MERGED_STAGES`) in a separate, behaviour-preserving commit *before*
   this pane widened the union from `frontier ∪ blocked` to every live
   item — without it, a captured-but-untriaged item would have started
   appearing on a weekend day because a different pane needed wider inputs.

`@homework` also fails CONTEXT.md's own test for a Context — *where or with
what* the work can be done — the test `@waiting` was deleted from the
suggested list for failing. That objection was raised during grilling and
**overruled by the operator**; CONTEXT.md's Context entry carries the
matching amendment, so the widening is on the record on both sides rather
than only here.

## Answer state

The first sort axis, and a closed three-arm vocabulary:

1. **answered** — the pane has an answer to give
2. **bound-but-unacquired** — a gap: the binding names a subject, nothing
   has been acquired for it yet
3. **unbound** — no binding

The vacation prototype shipped five arms and they remap onto these, which
is where the plan overrules the prototype:

| Prototype `AnswerState` | Maps to | Why |
| --- | --- | --- |
| `unbound` | unbound | — |
| `not_polling` | bound-but-unacquired | Device-local, but from the reader's seat it is the same fact: bound, nothing acquired. The *reason* is the pane's own copy, not a sort axis. |
| `no_snapshot` | bound-but-unacquired | — |
| `none_in_horizon` | **answered** | "Nothing booked in the next 90 days" is an answer, and must not sort with the gaps. |
| `answered` | answered | — |

`none_in_horizon` is the one that would have been got wrong by collapsing
the enum naively, and it is the reason the axis is three arms rather than
two: a question that answered *nothing* is not a question that *failed*.

**An unbound question still renders**, at the bottom, as its own setup
prompt. This overrules the vacation prototype, whose winner returns `null`
for `unbound` on the argument that "no Trips calendar designated is not an
error, it is a question nobody asked yet." True, and beside the point: a
question that renders nothing is a question nobody can discover, and the
binding editor is not somewhere a reader goes unprompted. Bound-but-
unacquired renders as a **gap**, not as absence (race prototype finding 5)
— adding a series to the binding must not look like it did nothing.

## Collapse

Every pane is collapsible and collapses itself by default in `dormant` and
in any state that cannot answer. The shell draws every collapsed row from
`collapsedHeadline` + icon; **panes never render a compact form of their
own.** The headline is computed on every rank, not only when dormant — it
is what the reader sees the instant they collapse a pane by hand.

The waste prototype's dormant rendering *is* its collapsed rendering: three
coloured bin dots, the weekday, and a day count, no card. The dots are the
one thing on that tile a person matches against the real world before
walking outside, and they encode **object identity** rather than status — a
deliberate, documented exception to the design system's "colour always
encodes status, never decoration" rule. So the contract's `icon` is not a
single glyph slot: it is a **bounded pane-supplied glyph group**, drawn by
the shell inside shell-owned row chrome and type. The shell still owns
everything that must not drift; the pane supplies only what the shell could
not have known.

**The collapse override is device-local and band-scoped.** Persisted
through the injectable-`storage` idiom of `calendar/persistence.ts` /
`theme/useTheme.ts`, and **discarded the moment the pane's computed band
changes**. A sticky override would keep a race pane collapsed from December
through the morning it goes `live` — the same class of bug as an alert that
never rings again. Deliberately *not* a `settings` binding: a view
preference is not a cross-device binding fact, and that table has no DELETE,
so an override map would accrete keys for panes that no longer exist.

## The Rust/TS carve-out

**This section is amended — see the Status header.** ADR-0025 redrew the
line for the multi-client world (band functions and their thresholds sink
into the core), and its own M1-1 amendment records the mechanism the web
reaches them through. What follows is what was decided here, in 2026-08-10's
one-client world.

Bespoke answer and band live in **TS `screens/*.ts` pure modules**, which
protects the visual iteration loop all four prototypes proved out. The
core-side read is generic — for a source, its snapshot rows and its live
alerts, with no knowledge of what they mean.

Two things stay in Rust because they must not drift:

**Alert liveness** — `hummingbird_domain::is_live`, ADR-0014.

**Freshness**, and it is deliberately *not* a boolean:

```
Freshness
  ├─ Unknown                                        no fetch stamp at all
  └─ Age { age_ms, declared_cadence_ms: Option }
```

Three arms, because two different unknowns exist and collapsing them hides
a real difference: `Unknown` means *we do not know the age*, while `Age`
with `declared_cadence_ms: None` means *we know the age but not what normal
looks like*. The invariant Rust enforces is that **`Unknown` may never
render as fresh** — the same rule `shell/sync-status.ts` already carries
("an outcome that did not run must never read as success"), and one the
vacation prototype quietly broke: `staleness(null, …)` returns
`{ label: null, stale: false }`. Harmless there only because `AnswerState`
said `no_snapshot` separately. At the type level it cannot be broken.

`age_ms` is clamped against a skewed device clock **once**, in one place,
under one stated clock rule. Two prototypes independently hand-rolled
`Math.max(0, now - fetchedAt)`; that is the drift the carve-out exists to
stop.

**The threshold is not carved out.** It stays in TS beside each band
function, because the driver is not the cadence — it is the **cost of a
wrong answer**, which differs per question. Race polls every six hours and
calls `2 ×` stale; waste polls daily and calls **26h** stale rather than
48h, because a 47-hour-old waste answer can be a whole collection cycle
wrong and would render "Trash Tonight" on the wrong night. A universal
multiplier of cadence would silently overrule that.

This also makes one type serve all four panes: the two snapshot-lane panes
take `declared_cadence_ms` from the envelope, the two calendar-lane panes
from the mirror's own poll interval. Rejected alternative: putting
`stale_after_ms` in the envelope as data. It cannot serve the calendar
panes — they have no envelope — so it would leave two mechanisms where one
suffices.

## The snapshot envelope

`context_snapshots.payload` gains a common envelope:

```json
{ "schema": "…", "polled_every_ms": 21600000, "body": { … } }
```

The core parses the envelope only and passes `body` through opaque. This
amends ADR-0009's "payload: JSON, source-shaped" — now *enveloped*,
source-shaped inside. It gives freshness a declared cadence (no cadence
column exists anywhere) and gives the known-to-be-provisional payload
shapes a version discriminator.

- Broken envelope → typed as malformed with a reason. Visibly broken, never
  quietly empty.
- Missing `polled_every_ms` → `Age { declared_cadence_ms: None }`. A
  legitimate state, not an error.
- Unrecognised `schema` → passed through; the pane renders its own "this
  device is behind" state.

`polled_every_ms` is not a new idea — it already exists in two prototypes
under other names (`RaceSnapshotRow.pollIntervalMs`, vacation's
`scenario.pollIntervalMs`). Two questions needed it independently before
anyone proposed sharing it.

## `alerts.subject_key`, and the additive join

`alerts` gains a **nullable `subject_key`** naming the snapshot `key` it
belongs to. The pane join is `(source, subject_key)` ↔ `(source, key)`.
`SCHEMA_VERSION` 3 → 4.

`source_key` stays **occurrence identity and nothing else** — never a join
key, never parsed. This reverses an earlier position in the session, taken
when panes were assumed to span a whole `source`.

The forcing case is the race pane and, today, only the race pane. One
`source` (`race-schedule`) carries one row per series (`f1`, `indycar`), so
joining on `source` alone shows every series' alert on every series' pane.
Recorded plainly because the ADR should not overstate its own evidence:

- **Waste deliberately does not join the alert lane at all.** Its verdict:
  a holiday is not an interruption laid over the answer, it *is* the answer,
  so it changes the words and there is nothing to ack away. The pane reads
  `collectedOn !== scheduled` off the snapshot. This is compatible — "a live
  alert may lift a band through the pane's own judgement" and waste's
  judgement is *never* — and the alert row is still minted, for the
  notification lane.
- **Vacation raises no alerts by construction.** There is no material change
  to report; the number goes down by one each day, on cadence.
- **Weekend is a pure mirror query.**

Two rules follow:

1. **`sweep_tick` leaves `subject_key` NULL.** Items are not standing
   questions and have no pane; `item-threshold/v1`'s `source_key` stays
   `item:<id>` as occurrence identity.
2. **The pane join is additive, never exhaustive.** An alert whose
   `subject_key` matches no pane is not dropped — it lives in
   `AlertsScreen` exactly as before. `subject_key` decides only whether a
   pane *also* shows it.

Rejected: **per-series source strings** (`race-schedule:f1`). It makes the
source vocabulary unbounded and runtime-generated, breaks ADR-0014's
version-suffix convention, and multiplies rows in the frozen registry for
what is one feed. Also rejected: **pane-level attribution** (each pane
filtering the source's alerts by parsing `source_key`), which is exactly
the "never parsed" rule this ADR is restating.

## Time

*This section is amended by a later ADR — see the Status header.*

**Device-local, everywhere, with no zone label.** This matches what the app
already does for owned dates: `screens/urgency.ts` resolves a day-only
deadline to `T23:59` naive local wall clock, with a twin in the domain
crate. The race prototype's hardcoded `America/Los_Angeles` and its "PT"
suffix on every time are prototype expedients and both go. The cost, taken
deliberately: a race appears to move when you travel. The benefit: the hour
on screen is correct for the device in the reader's hand, and matches every
deadline the same screen renders. Rejected: a `home-zone` binding, and a
fixed named zone — both make one question's times disagree with the rest of
the app's, and neither can be right for a reader who has actually moved.

This matters to the seam because bands and `withinBand` are day-derived for
three of the four panes: *which midnight* decides which band a pane lands in
on a boundary day.

**The invariant that makes device-local safe:**

> A civil date is never stored as an instant. It is stored as a civil date,
> and either compared against the reader's current civil date or resolved to
> an instant at read time in the reader's zone. An instant is stored as an
> instant and rendered in the reader's zone.

The app already obeys this for owned dates (`deadline`, `scheduled_date` are
naive text, converted per-render) and waste's payload obeys it (a civil
collection date). The one place it is violated is the one place still open
for decision: **both calendar-backed prototypes take `startMs`/`endMs` with
all-day events already flattened to local-midnight instants**, and both call
that shape a guess because #46's event shape is undecided.

That flattening is where device-local breaks. An all-day trip
`2026-09-09 → 2026-09-16` reduced to midnight-ms in whatever zone did the
flattening, then read one zone east, lands a calendar day early: *"India in
**394** days."* This is precisely the failure the vacation prototype built
`daysBetween` to prevent, arriving through the door `daysBetween` does not
guard. It also corrupts the phase machine, which computes
`lastDayMs = endMs - DAY` and fires `returns_today` off it — "Home today
from Lisbon" on the wrong day.

So **#46's event shape carries two arms**, discriminated by the `allDay`
flag both prototype shapes already have:

- **all-day** → `startDate` / `endDate` as `YYYY-MM-DD`, the provider's
  exclusive end preserved
- **timed** → `startMs` / `endMs` as UTC instants

*Clarification (not an amendment), added after #46 shipped:* the names above
are the **DTO's**, in the vocabulary of the two TypeScript prototypes this
passage is comparing — and `store/protocol.ts`'s `CalendarEventWhenDTO` carries
them exactly (`kind: "allDay" | "timed"`, `startDate`/`endDate`,
`startMs`/`endMs`). The client core's own wire is snake_case, per the
convention every tagged enum crossing that seam already follows
(`freshness.rs`, `rank.rs`, `bindings.rs`, `pane.rs`): `EventWhen` serializes
`{"kind":"all_day","start_date","end_date"}`, and `calendar-worker.ts`'s
`mapCalendarEventWhen` is the single rename point, exactly as `mapFreshness`
renames that type's tag key from `state` to `kind`. The decision here — two
arms, civil dates against instants, no stored zone — is what this ADR fixes;
the casing at each layer is not.

No source zone is stored on either. For a timed event the reader always
wants their own local day — "my plans this weekend" means *my* Saturday, and
a 23:00 Berlin dinner belongs on the reader's Friday if that is where they
are. The one case this cannot serve is a timed event you will attend in a
zone you are not in yet, which is unsolvable without knowing future location
and is the same limitation already accepted for race.

**Consequence: #46 becomes a blocker for the vacation pane as well as the
weekend pane.** It is currently listed only as weekend's.

## Placement, and the ranked region

The ranked region **owns Now's Context aside**. No legacy tiles, no fixed
block, no exceptions. **Standing questions never take the banner** — one
region, one slot.

This is an empirical result rather than an assertion, which is why the
prototypes were run in the real `NowScreen` rather than in isolation. All
four winners are context-panel tiles, and every banner bid lost on its own
merits: race B "competed with the top pick for the eye, for a question that
is rarely urgent"; vacation B "lost the slot"; weekend B was the only banner
variant and lost.

**The region scrolls.** `screens/layout.tsx`'s `Aside` is
`position: sticky; top: 0` with no `max-height` and no `overflow`, and the
weekend prototype already overflows it *by itself* — its condensed card runs
21 rows on a packed weekend and falls off the bottom. The ranked region now
holds the whole stack (races emits one pane *per series*, plus vacation,
waste, weekend, plus the interim calendar tile), and collapse-when-dormant
does not bound it: on a race weekend nothing is dormant.

**"What's on now / next" becomes its own standing question**, requiring its
own grilling and prototype — a new slice under #117, not this work. The
existing calendar tile stays as an interim until that lands.

## Registration and the sort

Registration copies the `shell/screens.ts` + `NavRail.tsx` idiom: a
`StandingQuestion` union, a `Record<StandingQuestion, QuestionDef>` for
exhaustiveness, and an ordered array for declared order.

The sort mirrors `orderFrontier`'s band → finer key → stable id:

1. **answer state** — answered, then bound-but-unacquired, then unbound
2. **band** — `live`, `imminent`, `near`, `distant`, `dormant`
3. **`withinBand`** — soonest moment first; null sorts after any pane that
   has one
4. **declared question order**
5. **subject key**

**The region re-sorts only on `syncOutcomeSeq` bump and on mount.** Pane
*content* stays optimistic and instant — the overlay makes a local write
visible immediately, exactly as every other surface gets it — but a pane's
*position* is sampled on ADR-0007's existing 60-second beat.

The hazard this closes is specific: the weekend pane's affordance is inline
day chips, so setting a do-date changes the overlay, which changes the
merge, which can change the band, which **moves the pane under the reader's
cursor mid-interaction** — the next chip is somewhere else. Re-ranking on
every read is simpler and wrong. Freezing rank while a pane has focus or an
in-flight mutation is also correct and costs machinery this does not need.

The cost, stated: a pane that goes `live` waits up to 60 seconds to move to
the top. That is the right trade — a ranked region that twitches is worse
than one that settles — and it has a second benefit: the band-scoped
collapse override is discarded on the same beat, so override and position
can never disagree about what band a pane is in.

## Writes from a pane

A pane may mutate, and one of the four winners does: the weekend card's
inline day chips set `scheduled_date`.

Every such write goes through **`Core::triage`** — a `TriagePatch`, one
queued CAS `PATCH` — and never invents an entry point. `Core` has exactly
three mutation entry points and all three enqueue through
`SyncCycle::enqueue`; a pane is not an exception to the durability rule.

There is **no pending-specific band rule.** The band is computed from the
same overlaid read every other surface uses, so an optimistic write moves it
exactly as a confirmed one would, and a 409 rebase or dead-letter moves it
back the same way. Whether "N due this weekend, no day chosen" should lift
the band is the weekend pane's own judgement, not a seam rule.

## Binding keys

Kebab-case and **unversioned**: `race-series` (already in the server
fixture, `handler_fixtures/settings.rs`), `trips-calendar`,
`city-waste-page`. Unversioned deliberately, so a `city-waste/v1 → /v2`
source bump does not orphan a binding in a table that has no DELETE.

## What this obliges

- **`server/`** — the `alerts.subject_key` column, `SCHEMA_VERSION` 4, the
  optional field on the ingest DTO, the frozen-DDL text in
  `handler_fixtures/schema.rs`, and envelope parsing for
  `context_snapshots`.
- **The source registry** — both lanes this ADR named are now registered.
  `city-waste/v2` enrolled at #254 (`Writes::Both`), and the race lane at
  #266 as `race-schedule/v1` (`Shape::Event`, `Writes::Both`,
  `Expiry::Always("the race's start time")`), which is also where the
  payload stopped being a prototype guess: the feed is Jolpica
  (`api.jolpi.ca/ergast/f1/current.json`, the maintained Ergast successor)
  and the body is `{events: [{name, locality, starts_at_ms, sessions: [{kind,
  label, starts_at_ms}]}]}` — epoch ms and no `zone`, because a race start
  is an instant rather than a civil date, and the whole season unfiltered,
  because "next" is a read-time answer. `server/race-poll`'s
  `tests/fixtures/golden-body.json` is that body's committed contract, which
  #119's pane parser is written against. The envelope's `schema` now has a
  legal value for both.
- **#46** — the two-arm event shape above, and it now blocks two panes.
- **#217** — unchanged and still open, but worth naming here: the
  `item-threshold/v1` resolution pass has no bearing on panes, because
  `sweep_tick` writes no `subject_key`.
- **Each pane slice (#119–#122)** rewrites its prototype rather than
  promoting it — every prototype was built under prototype rules, and each
  carries open questions its verdict did not close (the weekend window's
  Friday edge and its degenerate Sunday-night case; the vacation +90d
  horizon, landing day, and what the pane reads mid-trip — **all three of the
  vacation ones closed by #121, see the amendment below**; the race schedule
  source itself, closed at #266 on Jolpica before #119's pane starts).

## Amendment (2026-08-11, #121): the vacation pane's three open questions, answered

`screens/vacation-pane/` shipped, and it closes every question this document
left open for it. The prototype is deleted, not promoted.

**The +90d horizon → a per-calendar window, host-selected.** The flagship
example above ("India in 395 days") was outside the mirror entirely, and
nothing in the winning verdict noticed. ADR-0005's amendment of the same date
is the fix: `CalendarSelection { id, horizon }` replaces the bare id list
through the adapter, the poller and the worker protocol, and
`CalendarHorizon::Long` polls −7d/+730d where `Standard` keeps −7d/+90d. So
the empty answer names its own horizon — *"Nothing booked in the next 2
years"* — because the pane still cannot tell "genuinely nothing" from "beyond
what this device polls", and a bare "Nothing booked" would make an `answered`
state a lie.

**Landing day → the return day is still the trip.** The issue's "the day you
land home it is already counting to the next one" was loose phrasing, not a
decision: an all-day event's end is the provider's *exclusive* end (local
midnight after the last day), so the trip is live until then. Five phases —
`upcoming` / `departs_today` / `under_way` / `returns_today` / `past` — and
`returns_today` reads *"Home today from Lisbon"*.

**Mid-trip → the trip you are in leads.** *"In Lisbon · day 3 of 6"*, with the
queue below unchanged so the next trip is still one line down.
`collapsedHeadline` is free prose, so a pane that is a countdown ~94% of the
time and a status line the rest costs nothing structurally.

**And the `EventWhen` rule the "India in 394 days" section above now
enforces.** #46's two-arm shape carries an all-day trip as the provider's own
`start_date`/exclusive `end_date` strings, with **no instant and no source
zone**, while a timed trip carries only `start_ms`/`end_ms` instants. The pane
therefore reads all-day dates directly and resolves timed instants in the
**device's** zone — the same zone that decides **"today"** — then counts every
distance with `civilDaysBetween` over two `YYYY-MM-DD` values. Only the
all-day arm derives its last day as the exclusive end minus one **civil day**;
the timed arm keeps the real end instant's device civil date. `endMs - DAY`
appears nowhere. This makes the old failure mode unrepresentable: there is no
zone-resolved all-day midnight for a reader west of Kolkata to turn into the
previous day, and no carried zone to guess or reject.

**"Dormant is not a synonym for far away" is now load-bearing code.** Any
booked trip keeps this pane out of `dormant`, at 31 days or 731; dormant here
means *there is nothing to count to*. Rejected: `distant` decaying to
`dormant` past ~180 days, which would ship the flagship 395-day case collapsed
by default, inverting this document.

**Still true, and worth restating: the pane raises no alerts by construction**
— there is no material change to report, the number goes down by one a day on
cadence, and `subject_key` is unused.
