# ADR-0008: The authority is an app-owned server

**Status:** accepted · 2026-08-08
**Context:** the backend reconsideration of 2026-08-08 — a from-scratch
tradeoff analysis (Linear-stays vs owned server vs CRDT local-first) followed
by the authority-move grilling. Supersedes the authority designation of
[ADR-0001](0001-linear-is-the-authority-behind-a-clean-seam.md) via that ADR's
own migration trigger; the owned schema it serves is
[ADR-0009](0009-the-owned-schema-and-context-lanes.md).

## The fired trigger

ADR-0001 listed "the Issue model demonstrably fights a feature" as a
migration trigger. Verified live on 2026-08-08, it has fired, structurally:

- Linear's schema cannot hold the domain. Steps exist only as markdown
  checkboxes inside an opaque body string; Routes live in a project
  description. First-class entities were being serialized into the one field
  that is whole-string-PUT and multi-writer.
- Linear's public API has **no conditional write** (no version precondition
  anywhere in the schema) and **no delta API** for descriptions — its own
  collaborative editing is internal-only (Yjs). The lost-update window on body
  edits is permanent, and body editing had just been ruled in scope.
- Every future below-issue feature (Step ticking, Route editing, checklist
  reordering) would re-enter the same swamp.

The two costs that justified rejecting an owned backend in ADR-0001 have both
shrunk since: ops/durability is near-zero on 2026 infrastructure (see
substrate), and ADR-0001's own rule 3 already made every device a full export.

## Decision

**The designated authority for all task data is an app-owned server**: a thin
compare-and-swap API over the owned schema, written in **Rust** and deployed
as a **Cloudflare Worker with one SQLite-backed Durable Object**.

The one-authority doctrine survives ADR-0001 verbatim: exactly one designated
authority at any time, declared in code, migration a deliberate event. Only
the authority named has changed.

**No data is migrated.** The Linear workspace is not imported — the dataset
starts fresh. Linear winds down whenever the owned stack is daily-usable; no
Linear adapter is ever built in the client (the S2/S3 slices of #95 retarget
to the owned API).

### Substrate

- **Cloudflare Durable Objects, SQLite storage.** $0 at this scale on the
  free plan (worst case $5/mo Workers Paid); **30-day point-in-time recovery
  on by default** is the entire backup story; SQLite is the mandatory backend
  for new DO namespaces since 2026-07. A single DO instance is
  single-threaded, so CAS is a version check with no possible race.
- **Rust (`workers-rs`), sharing the domain crate with `client/core`.** The
  domain types are extracted from `client/core/src/task/` (S1, #94) into a
  standalone workspace crate both sides compile — schema agreement between
  client and server becomes a compiler guarantee. All server logic lives in a
  pure crate over a storage trait, fixture-tested natively (the
  `client/core` transport pattern); the `workers-rs` shim stays thin.
- **Same-origin:** `hb.twinion.net/api/*` routes to the API worker; the
  static shell keeps its worker (ADR-0006). No CORS anywhere;
  `connect-src 'self'`.
- **Commitment gate:** a walking-skeleton spike — one DO, one CAS write, one
  version-gated read, `wrangler dev` + CI. If `workers-rs` fights for more
  than a day, fall back to TypeScript + `ts-rs`-generated types on the same
  schema and API; nothing else changes.

### Writes: entity-level CAS, client rebases

- Every entity carries an integer `version`. Every mutation is an
  **absolute-value set** (never a relative change) plus `expected_version`.
- A stale write is rejected with **409 carrying the current entity**. The
  client compares the fields it touched: **disjoint → auto-resend** against
  the new version; **same field → the local edit loses** and lands in the
  client-side dead-letter journal ("1 edit didn't apply"), per ADR-0007.
  "Linear-wins" becomes "the authority adjudicates" — strictly stronger than
  client-side check-then-act, with the same user-visible semantics.
- **Creates are idempotent by client-supplied deterministic id**;
  already-exists is success. The sweeper's contract, now first-class instead
  of reverse-engineered from Linear error strings.

### Reads: delta pull, full sweep as backstop

- A workspace-wide monotonic version counter stamps every write. The normal
  pull is **changes since version N** — provably complete because rows are
  never deleted (archival is explicit data; ADR-0007's absence-inference
  dissolves) and every write bumps one counter. An unchanged workspace costs
  one row read, which also keeps the free tier's 5M rows-read/day distant.
- The **full sweep remains the correctness backstop** (on app open + daily),
  exactly the structure ADR-0007 prescribed for any incremental optimization.
- Everything else in ADR-0007 stands: one cycle (drain FIFO queue, then
  pull), atomic apply, exponential backoff capped at 5 minutes, 60-second
  foreground timer, event-driven triggers.

### Auth: ADR-0004's shape, ported

- **Per-writer long-lived bearer tokens** — one per device, one for the
  sweeper, one per ingest source — scoped (`device` / `sweeper` / `ingest`),
  sha256-hashed at rest server-side, individually revocable
  (ADR-0009's `tokens` table).
- Minted via `POST /api/admin/tokens` gated by an `ADMIN_SECRET` Worker
  secret used only from the operator's terminal. No OAuth, no rotation
  machinery — preserving the "device dark for weeks wakes up and drains its
  queue" property ADR-0004 chose personal keys to protect.
- Resting places port unchanged: IndexedDB (web), Keystore-backed storage
  (Android/Wear), Keychain (iPad). The host supplies the token at init; the
  core never persists it; **401 holds the queue** and surfaces a re-prompt.
- ADR-0004's revisit trigger carries over word-for-word: move to OAuth the
  day this serves anyone but its author.

### The sweeper

- **Off since 2026-08-08** (Fly machine stopped, healthchecks paused).
  Captures wait in their sources — Tasks items stay incomplete, Gmail labels
  stay on — and the frozen namespaces make the eventual drain duplicate-free.
- When the owned stack is daily-usable, one PR retargets the engine's write
  side to `POST /api/items` (create-in-authority-first; identical
  crash-safety proof). Adapters, acks, namespaces, healthchecks, quarantine,
  denylist and the Fly/supercronic substrate are untouched. The Google Tasks
  adapter stays for voice capture; Gmail stays; retirement is lazy and
  per-surface.

### Re-examined and standing

- **ADR-0005 stands:** calendars remain device-polled with host-owned OAuth.
  The deciding arguments (M365 daemon auth, per-device consent and
  revocation, freshest mirror on the device in hand) were
  authority-independent. Server-pollable sources with daemon-friendly static
  keys get their own lane in ADR-0009 instead.
- **ADR-0001's seam rules survive as doctrine** — they are what made this
  exit a repointed adapter instead of a rewrite, exactly as designed.

## Rejected alternatives

- **A — Linear stays the authority.** Keeps a polished fallback UI forever
  and zero backend ops, but the schema/CAS/delta faults above are confirmed
  permanent, the Q2/Q3 body-editing tax recurs on every below-issue feature,
  and the free-tier cap erodes the zero-cost advantage. The fallback-UI loss
  is the honest price of the move and was accepted deliberately (Q1 of the
  grilling: build the right thing rather than keep a half-fit one).
- **C — CRDT local-first (Automerge/Loro).** Real and Rust-friendly in 2026,
  but it moves the system onto a young substrate to solve a conflict problem
  that mostly dissolves once the schema fits the domain; it taxes every
  non-device writer (sweeper, skill-runner would need a headless core); and
  merge functions cannot enforce domain invariants, forfeiting
  authority-adjudication. A text CRDT stays in reserve for the `description`
  field alone if concurrent prose editing ever demonstrates demand.
- **Fly machine + axum + Litestream.** The most conventional shape, but
  dominated here: ~$2–4/mo plus a bucket credential plus restore discipline,
  versus PITR-by-default; and it reintroduces cross-origin CORS/CSP surface
  for the credential-holding client. Chosen only if the server ever needs to
  be a long-running process.
- **TypeScript server on DO.** The fallback if the `workers-rs` spike fails:
  same schema, same API, but the domain stops being one compiler-checked
  artifact and every future domain change becomes a two-language change.
