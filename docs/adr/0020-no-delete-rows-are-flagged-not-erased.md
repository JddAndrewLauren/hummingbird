# ADR-0020: No DELETE — rows are flagged, not erased

**Status:** accepted · 2026-08-13
**Context:** the delta-contract question asked and answered during #115, and
never written down. ADR-0009 states the rule in one line each in two
places — "Rows are never deleted, only flagged" in its schema header, and
`deleted_at INTEGER, -- flagged, never erased` on `steps` — without the
reasoning behind it. This ADR is that reasoning, cross-referenced from
ADR-0009 so the rule and its rationale are reachable from each other.
Cross-references [ADR-0007](0007-sync-is-one-cycle-drain-then-full-sweep.md)
for the sweep cadence and
[ADR-0016](0016-the-alert-horizon.md) for the horizon precedent this
decision leans on without repeating.

## Decision

**There is no DELETE route, on any synced table, anywhere in this system.**
A row that a user "removes" — a step, an item, a project — is flagged with
its own lifecycle column (`deleted_at`, `archived_at`, `removed_at`,
per-table per ADR-0009's schema) and kept forever. This is a decision being
*kept*, not merely a fact of the current schema, and it is worth recording as
one because reversing it is not a small change: see "What reversing it would
cost" below.

**Note (#394, 2026-08-13): two `DELETE` routes do exist, and neither
contradicts this.** `DELETE /api/admin/tokens/{id}` (`admin_tokens::revoke`) and
`DELETE /api/push_targets/{id}` (`push_targets::revoke`), both in
`handlers/mod.rs`, sit on `tokens` and `push_targets` — neither is a synced
table — and neither handler erases a row: each sets `revoked_at` and is
idempotent on a second call. They are evidence for this posture, not
against it.

### The mechanism

`GET /api/changes?since=N` (ADR-0009) transmits every row whose `version >
N`. The client's `apply_delta` is purely additive: it walks the rows it is
given and applies them, and it never infers a removal from a row's *absence*
from the response — there is no code path that asks "which rows did I have
before that aren't here now." A hard-deleted row has no `version` left to
carry across that gate; it simply stops appearing, and an additive applier
reads "stopped appearing" as "nothing happened," not as "gone." Only a full
`GET /api/sweep` — a complete table listing, which ADR-0007 puts on every
app open plus daily — compares the whole set and could ever notice a row's
disappearance, and it would notice it as an unexplained absence with no
timestamp and no reason, not as a delete event.

**The soft-delete flags are the tombstone mechanism.** `deleted_at`,
`archived_at`, and `removed_at` each carry a `version` bump like any other
mutation, so they ride the ordinary delta pull. A device that was offline
for a week and pulls the flag change learns the row was removed, and when,
through the exact same additive path that already handles every other
mutation. No second removal path exists or is needed, because flagging a row
*is* a normal write.

### What reversing it would cost

Hard-deleting rows — dropping the flag columns and issuing real `DELETE`
statements — is not free to add later, and the missing pieces are worth
naming so a future proposal is judged against the real cost rather than
against "just add a DELETE handler":

1. **An explicit per-table removal signal in `ChangesResponse`.** Once a row
   can vanish outright, the delta must say so, because an additive applier
   with no removal signal will keep rendering a row the authority no longer
   has. Every one of the ten tables' `apply_delta` arms in
   `client/core/src/sync/mirror.rs` would need to honor it, not just the one
   table that first wants hard deletes — the mirror has no per-table opt-out
   for "this table's absences matter."
2. **A tombstone horizon.** A removal signal cannot be carried on the wire
   forever without also growing monotonically (the exact cost soft-delete
   already pays, see below, now duplicated). So it needs an expiry: how long
   a deletion is transmitted before it drops off the delta, and therefore how
   far behind a device is allowed to fall before its mirror is silently
   wrong about what the authority still has.

### Why the horizon is the expensive half

[ADR-0016](0016-the-alert-horizon.md) already built exactly this shape once,
for `alerts`' wire horizon, and it is worth reading in full before proposing
a tombstone horizon here rather than re-deriving its findings. ADR-0016
documents a gap in that horizon that is inert in practice but not by
construction: the version cursor measures *write order*, never *wall-clock
age*, so a row whose settling stamp is already historical at write time (a
skewed clock, an offline-queued write, a backfill) is omitted from the very
delta that would have carried it, while the cursor advances past it anyway —
"stranding" a device on stale state that the sweep must silently correct.

A tombstone horizon for hard deletes would have the identical failure mode,
for the same reason: a horizon measured from wall-clock age against a cursor
that only tracks write order can always be defeated by a stamp that is
already old when it is first written. The one difference is scope — ADR-0016
pays this cost for one table (`alerts`) that already has no floor by
necessity (an unacked alert must ride the wire forever); a general
hard-delete tombstone would pay it for **every** table that adopts it, each
with its own horizon, its own const, and its own version of ADR-0016's
"accepted gap" writeup to justify why the gap is safe for that table.

### The cost of the current posture, stated fairly

The posture is not free either, and pretending otherwise would make this a
worse ADR than the one-line rule it replaces:

- **Anything written is permanent.** There is no undo below the application
  layer for a mistaken write, which is why
  [`server/scripts/smoke-prod.sh`](../../server/scripts/smoke-prod.sh) is
  deliberately read-only (#239, #240) and no automated test exercises a
  production write — a bug in a test that ran against production data could
  never be cleaned up by a DELETE, only worked around with more flags.
- **The store grows monotonically.** Every table only ever gains rows.
  ADR-0016 already lives with this for `alerts`, the one table with no floor
  at all. This is not a real cost today: ADR-0001's watchline is ~250 active
  items, low thousands at the horizon, and flagged-not-erased rows at that
  scale are noise against Cloudflare D1's storage tiers.

### The flip condition

The posture holds as long as no synced table's row count is both large and
growing faster than the cost above stays negligible. The obvious future
candidate is **`deliveries`** (ADR-0012): a row per notification send, with
no natural ceiling — but `deliveries` has no `version` column at all
(ADR-0012's DDL), so it is already outside the delta contract this ADR is
about. Pruning it needs a scheduled job with a straightforward age-based
`DELETE`, none of the machinery above (no removal signal, no tombstone
horizon), because no client mirror is watching its absence through the delta
in the first place.

**Prune what never synced before inventing a removal signal for what did.**
If a table that *is* in the delta contract ever reaches `deliveries`' shape —
numerous, machine-written, genuinely worthless after a while — that is a new
ADR, weighing the two costs above against that table's specific access
pattern, not a reason to revisit this one.

## Rejected alternative

- **Hard delete now, with a tombstone horizon from day one.** Rejected for
  the reasons above: it is strictly more mechanism (a removal signal every
  `apply_delta` arm must honor, plus a horizon with ADR-0016's proven failure
  mode) purchased against a storage cost that is not a real cost at this
  repo's scale. It would also make every future ADR-0016-shaped horizon
  decision a *default* instead of a deliberate, table-by-table exception —
  exactly backwards from "prune what never synced before inventing a removal
  signal for what did."

## Consequences

- No table in `server/domain` may gain a real `DELETE` statement against
  synced data without superseding this ADR.
- A future proposal to hard-delete a table must show that table's rows are
  both numerous and worthless with age (the `deliveries` shape) and must cost
  out the removal-signal-plus-horizon machinery against that table
  specifically, per ADR-0016's worked example, rather than assuming deletion
  is simpler than flagging.
- ADR-0009's two "flagged, never erased" lines point here for the reasoning
  they don't carry themselves.
