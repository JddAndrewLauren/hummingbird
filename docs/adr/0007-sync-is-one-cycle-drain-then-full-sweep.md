# ADR-0007: Sync is one cycle — drain, then full sweep; absence demotes

**Status:** accepted · 2026-08-08
**Context:** the sync-mechanics grilling of 2026-08-08, wayfinder map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35) ticket
[#57](https://github.com/JddAndrewLauren/hummingbird/issues/57). Fills in the
sync behaviour of [ADR-0003](0003-one-rust-sync-core-embedded-per-device.md)'s
core under [ADR-0001](0001-linear-is-the-authority-behind-a-clean-seam.md)'s
Linear-wins rule.

## Decision

### The sweep is full-mirror, and it is the mechanism

Every sync re-fetches the sweep window — **all non-archived issues in team
ION** (`includeArchived: false`) — and diffs against the mirror. No
incremental cursor, no tombstone protocol, no first-sync special case: the
dataset is capped by design (ADR-0001's 250-active-issue watchline; low
thousands at the horizon), so a full sweep is a few paginated GraphQL pages,
trivially inside Linear's ~1,500 req/hr limit at any cadence chosen here.

- **Deletions and archivals are detected by absence** from a complete sweep.
- **The sweep is self-healing:** a missed update, adapter bug, or clock skew
  is corrected next sweep, not silently forked into the mirror.
- An `updatedAt` filter may be layered on later **as an optimization only**,
  with the periodic full sweep remaining the correctness backstop.

### Absence demotes, never deletes

An entity in the mirror but missing from a complete sweep is marked
**`absent`** (Linear-side archived, trashed, or moved) with a timestamp. It
drops out of all working views but **stays in the snapshot forever** — the
mirror only grows or updates; reconciliation never erases a record. A
formerly-absent id returned by a later sweep simply rejoins the live set.

This is what keeps full-sweep-by-absence compatible with ADR-0001's "the
mirror is the export": Linear auto-archives completed issues after a few
months, and without this rule the local copy would shed history as Linear
archives it.

### One cycle: drain the queue, then sweep

Every sync opportunity runs the same cycle — **outbound queue first, sweep
second** — so a device's own writes are reflected in the truth the sweep
brings back, and a still-queued edit is never flagged as a conflict against
pre-write server state.

- **The queue is strict FIFO** (the sweeper's create-first ordering,
  generalized). A *retryable* failure (network, 5xx, 429) blocks the queue
  and ends the cycle early; nothing skips ahead, so create-before-update
  ordering holds trivially. A *permanent* failure (a 4xx retry can't fix,
  except 401) moves to the dead-letter journal and the queue advances.
- **The sweep applies atomically or not at all:** pages accumulate in memory;
  only a complete fetch replaces the mirror, persists the snapshot, and
  publishes to the UI in one commit. A mid-pagination failure discards the
  partial sweep — consistent-but-stale, fixed by the next trigger.
- **Backoff is exponential with jitter, capped at 5 minutes**, and reset by
  any user-facing trigger, so a gesture always gets an immediate attempt.
- **401 is not a failure of the cycle:** queue holds, polling holds,
  credential-needed event to the host, per
  [ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md).

### Cadence is event-driven first, timer second

- Sweep **on app open / core start**, **on reconnect**, **on window focus**,
  and at the tail of every queue drain (built into the cycle).
- **Foreground timer: 60 seconds**, paused entirely while hidden or
  backgrounded. Worst case ~60 req/hr — leaving ample headroom for the
  context polling ADR-0005 placed in the same core.
- **Manual refresh is the same cycle**, user-invoked; no special path.
- **Native background scheduling is deferred:** the core exposes the cycle;
  Android / Wear / iPad hosts schedule it per-platform when those clients
  exist.

### Conflicts: field-level, and losers are journaled

Each queued mutation records the entity's **base `updatedAt`** and the **set
of fields it touches**. Creates never conflict — deterministic client ids,
create-first, idempotent retries (ADR-0001).

When newer server truth exists for an entity with a pending mutation:

- **Disjoint fields changed** → send anyway. There is no real conflict, and
  holding back loses an edit for nothing.
- **The same field changed** → the local mutation **loses, per Linear-wins**:
  pulled from the queue, never sent. Blind-sending would make the client the
  de-facto authority.
- **Losing ≠ vanishing.** The dropped mutation lands in a small
  **dead-letter journal in the mirror** (entity, field, local value, server
  value, timestamp), surfaced as a low-key affordance — "1 edit didn't
  apply" — for manual re-apply. This generalizes the additive-parse
  invariant: reconciliation may discard an *effect*, never *content* the
  user produced.
- Pending mutations render as an **optimistic overlay** on the mirror, so
  the UI never flickers backwards mid-queue.

## Rejected alternatives

- **Incremental cursor as the mechanism** — cannot see deletions without a
  tombstone protocol, and a cursor bug silently forks the mirror until
  noticed. Wrong trade at a dataset size where the full sweep is nearly
  free.
- **Hard-delete on absence** — quietly breaks "losing Linear is an
  inconvenience, not data loss" as Linear auto-archives history.
- **Pure last-write-wins on the queue** — simpler, but an offline edit can
  silently clobber a desk edit; and blind-sending stale writes makes the
  client the authority in practice, against ADR-0001.
- **Skip-and-continue queue draining** — drains faster but forfeits the
  trivial ordering proof that FIFO-with-blocking gives create/update/complete
  chains.
- **A background timer while hidden** — the web client has no reliable
  background execution, and a personal task list changes mid-session only by
  the user's own hand (covered by queue-drain) or the sweeper (~15-min
  granularity anyway).
