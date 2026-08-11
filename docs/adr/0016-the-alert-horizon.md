# ADR-0016: The alert horizon

**Status:** accepted · 2026-08-11
**Context:** the alert-pruning grilling of 2026-08-11, opened on #155.
Discharges the deferral
[ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md) took
by name ("Pruning acked alert rows (#155)… needs its own decision —
predicate, tombstone-versus-delete, and who runs it — and explicitly not a
TTL"). Amends [ADR-0007](0007-sync-is-one-cycle-drain-then-full-sweep.md)
and [ADR-0008](0008-the-authority-is-an-app-owned-server.md), both of which
call the sweep *complete*. Upholds
[ADR-0012](0012-the-notification-lane.md) unchanged. New glossary term
**Horizon** lands in `CONTEXT.md`.

`alerts` is the one synced table with no floor. Nothing ever deletes a row:
an acked alert keeps its `dismissed_at`, an expired one keeps its row, and
`expires_at` is deliberately absent for mail (ADR-0014: *"an email has no
such moment; it sits until acked — that is the contract, not a leak"*). So
the table grows monotonically for the life of the system.

The delta pull is version-gated, so steady-state sync is unaffected. The
cost lands on **`GET /api/sweep`** — ADR-0007's full-sweep correctness
backstop, which fires on every app open plus daily and carries every row
every time — and on a cold client's first sync.

## The invariant

> A live alert rides every sweep forever, at any age. A settled alert leaves
> the wire once **every** stamp that settled it is more than 90 days old.
> Its row never leaves the Durable Object.

Settled-**and**-old, never old. This is not a TTL, which is exactly what
ADR-0012 and ADR-0014 each rejected by name: a live alert is *never* behind
the horizon, whatever its age, because the wire is how a device learns the
alert is still standing and still wants an ack.

## The nine decisions

| # | Question | Decided |
|---|---|---|
| 1 | Floor in storage or wire? | **Wire horizon.** Rows are never deleted. |
| 2 | Age measured from? | **The settling stamp**, not `raised_at`. |
| 3 | How long, owned by whom? | **90 days**, a named `const` — never a `settings` row. |
| 4 | `max` or `min` of the stamps? | **`max`** — "every applicable stamp is older than 90 days". |
| 5 | Predicate expressed where? | **Rust filter** calling `hummingbird_domain::is_live`. One spelling. |
| 6 | Client told about the horizon? | **No.** Silent; the obligation is recorded below. |
| 7 | Missing `alerts(version)` index? | **Separate issue.** |
| 8 | ADR disposition? | **This ADR**, amending 0007/0008. |
| 9 | Build? | **Now**, in the one PR that closes #155. |

**The horizon is a `const`, not a setting** (Q3). `settings` is
device-writable and has no DELETE, so a mistaken `0` would be an
unrecoverable change to what every device can see — and there is no reader
who benefits from tuning it. It sits beside `ALARM_INTERVAL_MS`'s
precedent: a readable named constant rather than a buried literal.

**The age is the settling stamp, not the raise** (Q2). An alert raised two
years ago and dismissed yesterday is a *yesterday* fact about a device that
may not have pulled it yet; anchoring on `raised_at` would hide the
dismissal itself.

**`max`, not `min`** (Q4). `is_live` is a conjunction, so the strict reading
of when dormancy began is the *earliest* settling stamp. `max` is the
conservative reading — "every applicable stamp is old" — which errs toward
carrying a row, and survives a future fourth lifecycle column unchanged.

## Why the predicate is Rust and not SQL (Q5)

Two findings decided it, recorded here so the reasoning is not re-derived
the next time someone reaches for a `WHERE` clause:

1. **Cloudflare bills rows *scanned*, not rows returned**
   ([D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/),
   which DO SQLite matches): a query scanning a whole table to return one
   row is billed for every row.
2. **`alerts` has no index at all.** `schema.rs`'s `CREATE_INDEXES` covers
   `items`, `steps`, `rules` and three others; `alerts` is absent.

Together: a SQL `WHERE` reads **no fewer rows** than a Rust filter, because
SQLite scans every row to evaluate it either way — and the predicate (a
disjunction over a computed `max` of three nullable columns) is not servable
by any simple index. So the SQL option's only advantage evaporates, leaving
its cost: a second spelling of the live predicate, which ADR-0014 forbids by
name (*"written once, in `domain`… no consumer re-spells it in SQL"*).

Rows-read headroom is ~3 orders of magnitude (5M/day free tier vs. ~150k/day
at 20 app-opens against a year of alerts), so the optimization is not needed
in the first place.

## Where it is applied

`hummingbird_domain::settled_at` is the new sibling of `is_live`, in the one
module ADR-0014 designates as the single spelling of the live predicate:
same argument list, same clock rule, and `settled_at(..).is_none()` is
*exactly* `is_live(..)` by construction — it asks `is_live` rather than
re-deriving the three clauses, and a test pins the equivalence over
`is_live`'s own matrix.

The filter sits in `changes_since`, the one code path
`GET /api/changes` and `GET /api/sweep` already share, so the delta and the
sweep cannot diverge on it and their byte-for-byte agreement holds by
construction. `pull()` itself is untouched — it stays the uniform
ten-table function.

**Applying it to the delta too is free, not a compromise.** A settled alert
only appears in a delta if its `version` exceeds the cursor, meaning it was
written recently, meaning its settling stamp is recent, meaning it passes
the horizon. The filter is inert on the hot path; the only rows it ever
removes are ones a far-behind device is pulling, where removing them is the
point.

## What this amends

**ADR-0007 and ADR-0008 are narrowed.** ADR-0007's "deletions detected by
absence from a **complete** sweep" and ADR-0008's "provably complete because
rows are never deleted" now hold for every table *except* `alerts`, which
appears partially. `apply_sweep`'s absence-demotion
(`client/core/src/sync/mirror.rs`) stays sound: an alert the horizon excludes
is settled by definition, so it renders nowhere — and the client mirror
*retains* the record it demotes rather than dropping it.

**ADR-0012 is upheld unchanged.** Settled-and-old is what makes this not the
blanket TTL that ADR rejected: an unacked alert rides the wire forever.

**ADR-0014's deferral is discharged** — this is the decision it asked for.

## The accepted gap, recorded not hidden

A horizoned sweep is silent: an empty alert history reads as "nothing
happened" rather than "history starts 90 days ago". That is exactly the
shape this repo elsewhere refuses — `screens/questions/contract.ts`'s three
answer states exist because *a gap is not an absence*, and
`Freshness::Unknown` may never render as fresh.

It is accepted here because **nothing reads settled alerts today**:
`screens/AlertsScreen.tsx` is demo-fixture-only, and both
`SyncMirror::all_alerts` and `alerts_for_source` filter to live. Adding a
field to `ChangesResponse` later is backward-compatible, so deferring costs
nothing.

**The binding obligation:** whoever builds a real alerts-history screen adds
`alerts_horizon_ms` to `ChangesResponse` **in that same PR**, so the screen
can say where its history starts rather than implying it starts nowhere.

## Rejected alternatives

- **Hard-deleting settled rows.** The obvious floor, and the only one that
  reclaims storage — which is not the cost being paid here. It destroys the
  audit trail permanently, makes the decision irreversible (a horizon can be
  widened tomorrow; a delete cannot be undone), and puts a `DELETE` in a
  system whose entire sync contract, ADR-0003 onward, is built on rows never
  disappearing.
- **An `archived_at`-style tombstone column on `alerts`.** Reuses a pattern
  the schema already has, and buys nothing the wire filter doesn't: it costs
  a `SCHEMA_VERSION` bump, an `ALTER TABLE`, a DO migration and a writer to
  stamp it — plus a *second* piece of lifecycle state that can disagree with
  the three stamps that already decide liveness.
- **Anchoring the age on `raised_at`.** Simpler (one column, never null),
  and wrong: an alert raised two years ago and dismissed yesterday would drop
  off the wire the moment it settled, before a device that had been offline
  ever saw the dismissal.
- **The predicate as a SQL `WHERE` clause.** See Q5 above — no rows-read
  saving, and a second spelling of the live predicate.
- **A `domain`-owned SQL string plus an agreement test.** The honest version
  of the SQL option: keep one *source* of the predicate by exporting its SQL
  text from `domain` and pinning agreement with the Rust function in a test.
  It still ships two implementations that must be proved equal, for a
  performance win that was already shown not to exist.
- **A horizon on `sweep` only, leaving the delta unfiltered.** Reads as the
  cautious choice, since the sweep is where the cost lands. It creates the
  one thing this design most wants to avoid: two read paths whose contents
  differ, and with them the loss of #114's byte-agreement criterion. The
  delta filter is inert anyway, per "applied to the delta too is free".
- **Making 90 days a `settings` row.** See Q3.

## What this obliges

- The horizon is `ALERT_HORIZON_MS` in
  `server/authority/src/handlers/changes.rs`. Changing it is a code change
  and a deploy, deliberately.
- Nothing may re-spell the live predicate. A future consumer that needs "when
  did this settle" calls `hummingbird_domain::settled_at`.
- The alerts-history screen, whenever it is built, ships `alerts_horizon_ms`
  in the same PR.
- The missing `version` indexes (`alerts`, `projects`, `routes`, `fog`,
  `blocked_by`, `context_snapshots`, `settings` — a uniform seven-table gap)
  are their own issue. This decision was chosen partly *because* it is
  index-independent; bundling the index here would make it look load-bearing.
