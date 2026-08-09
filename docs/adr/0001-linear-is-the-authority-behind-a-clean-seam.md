# ADR-0001: Linear is the authority, behind a clean seam

**Status:** accepted · 2026-08-07 · **superseded in part 2026-08-08 by
[ADR-0008](0008-the-authority-is-an-app-owned-server.md):** the authority
designation moved to the app-owned server via this ADR's own migration
trigger ("the Issue model demonstrably fights a feature"). The one-authority
doctrine and the four seam rules survive — they are what made the exit cheap.
**Context:** the Linear-role grilling of 2026-08-07; wayfinder maps
[#1](https://github.com/JddAndrewLauren/hummingbird/issues/1) and
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35).

## Decision

Linear (org `twinion`, team `ION`) is the designated authority for all task
data, indefinitely. Every client holds a reconciling local mirror — a working
copy. Sync is real but asymmetric: Linear wins conflicts.

There is exactly one designated authority at any time, the code declares which
it is, and migrating authority is a deliberate event — never drift, never a
peer-to-peer both-accept-writes arrangement.

## The seam

So that a Linear exit is a clean event — a promoted replica plus a repointed
adapter, not a rewrite — four rules bind all client and sync code:

1. **The app's schema is the domain model** (`CONTEXT.md`: Action, Route,
   Step) — never Linear's Issue shape. Linear field names, state ids, and
   label ids appear only inside the adapter.
2. **Clients talk to one storage/sync interface**; the Linear adapter
   implements it. There are two seams total: clients ↔ sync engine, and
   sync engine ↔ Linear.
3. **The mirror is the export.** Every device's reconciled replica is a full
   copy; losing Linear is an inconvenience, not data loss.
4. **The authority is a single explicit declaration** the sync layer reads —
   "one authority, migration is deliberate" is enforced, not remembered.

## Writes

Outbound writes use the sweeper's proven pattern (`docs/sweeper.md`): an
outbound queue, deterministic client-supplied v4-shaped ids, create-first
ordering. Retries are idempotent; a crash produces a visible duplicate
attempt, never silent loss.

## Migration triggers

Execute the exit — or consciously decide to pay instead — when any of these
fires:

- Linear's free tier or API materially changes.
- The Issue model demonstrably fights a feature.
- The 250-active-issue cap forces the paid tier and paying (~$8/mo) feels
  wrong.

The active-issue count is an ongoing concern to watch, not an immediate
blocker.

## Rejected alternatives

- **App-owned backend now** — prepays ops, durability, backups, and the loss
  of Linear's UI as the standing fallback client, on desire-driven evidence.
  The sync client is nearly identical either way; only the authority moves.
- **Thin live client, no local store** — fails the hard requirement that
  capture *and* reads work offline.
- **Deliberately undecided** — undecided-without-discipline is drift, which
  is exactly what the one-authority rule forbids. This ADR is the decided
  version: authority named, seam enforced, triggers listed.
