# ADR-0002: Sources join by role; urgency is computed at read time

**Status:** accepted · 2026-08-07 · **amended 2026-08-08 by
[ADR-0009](0009-the-owned-schema-and-context-lanes.md):** the task authority
is now the owned server (ADR-0008), and context gains transport lanes —
server-polled snapshots and pushed alerts. Both taxonomies and rules 1–6
stand; alerts re-entered through this ADR's own petition mechanism.
**Amended 2026-08-09 by [ADR-0011](0011-context-ingestion-moves-server-side.md)
/ [ADR-0012](0012-the-notification-lane.md):** the petition mechanism admits
two more out-of-scope entries — M365 mail and the pager/alert-router role
(the notification lane is the demonstrated consumer). The urgency doctrine is
**reaffirmed**, with its cleanest formulation to date: rules evaluate at fire
time and stamp severity only on the alerts they mint; nothing ever writes
urgency onto an existing record; ranking is a read-time query over lifecycle
state.
**Amended 2026-08-12 by
[ADR-0019](0019-the-gmail-capture-unit-is-the-conversation-the-key-stays-the-message.md):**
the Placements table's Gmail row said the message is the unit; the decided
capture unit is now the **conversation**, with the message staying only the
identifying key.
**Context:** the data-sources grilling of 2026-08-07, issue
[#43](https://github.com/JddAndrewLauren/hummingbird/issues/43); extends
[ADR-0001](0001-linear-is-the-authority-behind-a-clean-seam.md).

## Decision

Two taxonomies, on two planes. They must never be collapsed into one.

**The storage plane classifies sources.** ADR-0001's one-authority rule is
scoped to the task domain and generalized to one authority per domain: Linear
owns tasks and calendars own scheduling. Sources play one of two roles defined
in `CONTEXT.md`: **capture** or **context**. The drain invariants,
deterministic ids, and ownership questions attach here.

**The attention plane classifies items, at read time.** **Urgency** is a
derived, time-varying property computed fresh by every consumer over the
mirror. It is never a stored class and never an ingestion-time route: the same
calendar event is background context at T−3 days and constrains
`/next-up-personal` at T−10 minutes without any record moving anywhere.

## Rules

1. **No context materialization, ever.** No context-source record becomes a
   Linear issue by machine. Context records have no stable consumed state for
   the drain invariants to attach to; a copy would be a second authority.
2. **No machine minting.** The only path from any non-capture item to a task
   is a human gesture — made one-click cheap: the gesture itself is a capture
   (unit = the item, `source_key` = its id, deterministic id makes a
   double-click an already-exists no-op).
3. **The adapters decided here are read-only.** Calendar writes, time-blocks,
   and actuation require a separate decision based on demonstrated desire.
4. **The mirror is derived and disposable** — the unified local read model of
   all authorities, and the dashboard's backing store. Deleting it loses
   nothing. Linear wins conflicts for task records, per ADR-0001. Context
   records are replaced from the authority in their domain and are never
   written back from the mirror by these adapters.
5. **One drain engine, isolated adapters.** A single sweeper run iterates
   capture adapters (`NAMESPACE` / `enumerate` / `derive_capture` / `ack`);
   one adapter's failure never stops another's drain. One frozen `NAMESPACE`
   per source, each guarded by its own frozen test vector; `google-tasks/v1`
   untouched.
6. **One healthcheck per capture source** — the #24 lesson generalized: a
   shared check held red by one broken drain hides the health of the others.
   Context pollers get no healthchecks; a stale "as of…" tile is their alarm.

   *Amended 2026-08-12 (#328): the taxonomy now admits one check belonging to
   no source — authority reachability. Both lanes ping their own check green
   on an empty drain, having made no authority call at all, and since the
   2026-08-12 go-live an empty drain is the steady state for both; a per-source
   green therefore stopped being evidence the authority is reachable. The
   fix is a third check, owned by no capture source, that probes the
   authority directly once per sweep and reports only itself — per-source
   isolation is otherwise unchanged: this check's failure never touches
   either adapter's result, and either adapter's failure never touches it.*

## Placements

| Source | Role(s) |
| --- | --- |
| Google Tasks | capture (built; fail-open denylist) |
| Gmail | capture (fail-closed: dedicated label is the gesture, unlabel is the ack, the **message** is the unit) |
| Google Calendar | context — the record archetype; read-only |
| M365 calendar | context — sequenced after Google Calendar (second auth stack: MS identity + Graph) |

*Amended 2026-08-12 by [ADR-0019](0019-the-gmail-capture-unit-is-the-conversation-the-key-stays-the-message.md)
(see the Status header): the Gmail row above is superseded — the capture
unit is the conversation, and the message stays only the identifying key.*

## Consumer order

Google Calendar lands first and supplies the provider-neutral mirror contract.
`/next-up-personal` is the first live consumer: current and next events constrain
the size of the task it recommends but never become Actions themselves. The
same interval query is the decided input for a future morning brief; authoring
that surface is separate work. M365 calendar then joins the same contract, last,
so neither consumer needs provider-specific logic.

## Out of scope

Named so scope creep has something to bounce off: telemetry and alert sources
(Fly, Supabase, Cloudflare, NAS, smart home, website status, provider usage
limits); hummingbird as pager/alert-router; Gmail inbox-state summaries; M365
mail; chat platforms (Teams, Slack, Discord); notes (the Obsidian vault is a
thinking surface, not an inbox); browser state / read-later; health, finance,
location, photos-as-capture, and media state. Each can petition back in through
the taxonomy when a consumer and a demonstrated desire exist. The "capture
calendar" exception (creating an event *as* the gesture) and calendar write-out
are deferred until the desire is demonstrated.

## Rejected alternatives

- **Materializing calendar events as issues** — recurring, edited records have
  no stable consumed state; every edit re-mints; a second copy of
  authoritative data.
- **Urgency as an ingestion-time route** (an "alerts path" for urgent items) —
  urgency is time-varying, so routing on it forces items to migrate between
  paths as the clock ticks, recreating the no-stable-state mess one level up.
  It also silently drops the ownership/consumability dimension the drain
  invariants attach to.
- **One script per capture source** — copies the engine, and the engine is
  where the load-bearing invariants live; drift in copies is the worst drift.
- **A local "unified source of truth"** — a both-accept-writes arrangement on
  your own device; ADR-0001 forbids it. The mirror keeps every benefit
  (unified reads, offline, one thing for a local model to act against) with
  none of the authority confusion.

## Sequencing

Three end-to-end adapter specs implement the decision in order:

1. [Gmail capture](https://github.com/JddAndrewLauren/hummingbird/issues/45),
   including its isolated drain path and dedicated healthcheck.
2. [Google Calendar context](https://github.com/JddAndrewLauren/hummingbird/issues/46),
   including the mirror and consumer contracts it needs.
3. [M365 calendar context](https://github.com/JddAndrewLauren/hummingbird/issues/47)
   on the same contracts, last.
