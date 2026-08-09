# ADR-0009: The owned schema — first-class domain records, and context joins by transport

**Status:** accepted · 2026-08-08 · amended 2026-08-09 (standing-questions
grilling: `scheduled_date`, `settings`, the standing-questions section, and
rule 2 generalized to material snapshot changes) · **amended 2026-08-09 by
[ADR-0011](0011-context-ingestion-moves-server-side.md) /
[ADR-0012](0012-the-notification-lane.md):** rule 3 is scoped to push
sources and gains a second clause for streams hummingbird polls itself; the
server-polled lane gains **evaluated streams** (mail and calendar events,
judged in-poll, only matches persisting); `alerts.severity` may be stamped
by the minting rule; and `rules`, `push_targets`, and the delivery log join
the schema (DDL in ADR-0012). · **amended 2026-08-09 by
[ADR-0013](0013-the-rule-condition-vocabulary.md):** `items.due_date` is
renamed `deadline` and widened to carry an optional time
(`2026-08-15T14:30`), validated in `domain`; provenance rule 4's common core
is generalized into the Event shape every rule sees. · **amended 2026-08-09
by [ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md):**
every `source` string carries a version suffix uniformly (`healthchecks/v1`,
not `healthchecks`), so the namespace bump is always available as the
sanctioned way to change a `source_key` recipe; and an alert is **live**
when `dismissed_at IS NULL OR raised_at > dismissed_at`, which is what lets
a state source ring again after an ack without a machine ever writing the
human-owned column.
**Context:** the authority-move grilling of 2026-08-08. Companion to
[ADR-0008](0008-the-authority-is-an-app-owned-server.md); amends
[ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md)'s
taxonomy with transport lanes. Replaces the planned "how a Linear Issue maps
to the domain model" ADR (issue #96), which is obsolete: there is no foreign
shape left to map. Issue #97's planned "ADR-0009" (one writer per origin)
renumbers to ADR-0010.

## Decision

**The schema is the domain model, literally.** ADR-0001 seam rule 1 said the
app's schema must be the domain model; under an owned authority that stops
being a translation discipline and becomes the database itself. Item, Step,
Route, Project and Fog are records. Ticking a Step is a scalar CAS write —
the operation whose impossibility under Linear triggered ADR-0008.

### The schema

```sql
-- meta: the workspace version counter (one row), bumped by every write.
-- Every mutated row stamps its `version` from this counter; the delta pull
-- is "WHERE version > ?" per table. Rows are never deleted, only flagged.
CREATE TABLE meta (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  version        INTEGER NOT NULL,
  schema_version INTEGER NOT NULL
);

CREATE TABLE projects (
  id          TEXT PRIMARY KEY,             -- uuid, client-supplied
  name        TEXT NOT NULL,
  archived_at INTEGER,                      -- ms epoch; NULL = live
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL              -- CAS target + delta cursor
);

-- Route: 1:1 with project, separate table because /to-actions owns it
-- (glossary: Destination, Fog, Notes, ordered actions).
CREATE TABLE routes (
  project_id  TEXT PRIMARY KEY REFERENCES projects(id),
  destination TEXT,
  notes       TEXT,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
);

-- Fog: segments not yet definable as actions, each with its open question.
CREATE TABLE fog (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  question    TEXT NOT NULL,
  position    INTEGER NOT NULL,
  resolved_at INTEGER,
  version     INTEGER NOT NULL
);

CREATE TABLE items (
  id          TEXT PRIMARY KEY,             -- uuid; the sweeper's deterministic ids land here
  seq         INTEGER UNIQUE,               -- HB-42 display handle, server-minted
  title       TEXT NOT NULL CHECK (length(title) > 0),
  description TEXT,                         -- the ONLY free-prose field; never holds Steps
  stage       TEXT NOT NULL CHECK (stage IN
                ('triage','grilling','ready','in_progress','blocked','done')),
  size        TEXT CHECK (size IN ('quick','short','deep')),
  energy      TEXT CHECK (energy IN ('low','medium','high')),
  context     TEXT,                         -- '@computer', '@calls', … free vocab
  priority    INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
  project_id  TEXT REFERENCES projects(id),
  project_pos INTEGER,                      -- order within the Route's action list
  deadline    TEXT,                         -- ISO deadline, set deliberately at triage only:
                                            --   consequences; the only date urgency reads.
                                            --   'YYYY-MM-DD' or 'YYYY-MM-DDTHH:MM' (naive
                                            --   local, minute precision) — ADR-0013
  scheduled_date TEXT,                      -- ISO do-date the human chose: a preference,
                                            --   slides freely, never feeds urgency
  source      TEXT,                         -- frozen namespace, always versioned (ADR-0014):
                                            --   'google-tasks/v1', 'gmail/v1', 'web/v1', …
  source_key  TEXT,                         -- the id in that source
  source_url  TEXT,                         -- deep link back: Gmail thread, Calendar event, …
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
);

CREATE TABLE steps (
  id         TEXT PRIMARY KEY,
  item_id    TEXT NOT NULL REFERENCES items(id),
  body       TEXT NOT NULL,
  done       INTEGER NOT NULL DEFAULT 0,    -- ticking = one scalar CAS write
  position   INTEGER NOT NULL,
  deleted_at INTEGER,                       -- flagged, never erased
  version    INTEGER NOT NULL
);

CREATE TABLE blocked_by (                   -- native sequencing edges
  item_id    TEXT NOT NULL REFERENCES items(id),
  blocker_id TEXT NOT NULL REFERENCES items(id),
  version    INTEGER NOT NULL,
  removed_at INTEGER,
  PRIMARY KEY (item_id, blocker_id),
  CHECK (item_id <> blocker_id)
);

-- Pushed context: discrete events raised at the app from outside.
CREATE TABLE alerts (
  id           TEXT PRIMARY KEY,
  source       TEXT NOT NULL,               -- frozen namespace, always versioned:
                                            --   'healthchecks/v1', 'home-assistant/v1', …
  source_key   TEXT NOT NULL,               -- names the thing (state source) or the
                                            --   occurrence (event source) — ADR-0014;
                                            --   re-raise upserts
  title        TEXT NOT NULL,
  body         TEXT,
  url          TEXT,
  severity     TEXT,
  raised_at    INTEGER NOT NULL,
  resolved_at  INTEGER,                     -- the source said it's over (infra up-event)
  dismissed_at INTEGER,                     -- the human waved it away (email, HA)
  expires_at   INTEGER,                     -- source-declared lifetime; read by the live
                                            --   predicate, never written back (ADR-0014)
  version      INTEGER NOT NULL,
  UNIQUE(source, source_key)
);

-- Server-polled context: gauges replaced wholesale each poll, never drained.
CREATE TABLE context_snapshots (
  source     TEXT NOT NULL,   -- frozen namespace, always versioned (ADR-0014):
                             --   'anthropic-usage/v1', 'github-hummingbird/v1', …
  key        TEXT NOT NULL,   -- metric within the source: 'weekly_limit', 'open_prs', …
  payload    TEXT NOT NULL,   -- JSON, source-shaped; clients render tiles from it
  fetched_at INTEGER NOT NULL,-- drives the "as of…" staleness display (ADR-0002's alarm)
  version    INTEGER NOT NULL,
  PRIMARY KEY (source, key)
);

-- Workspace preferences: small cross-device binding facts (which race
-- series are followed, which calendar is the vacations calendar), synced
-- through the normal delta pull. Machinery like meta and tokens, not a
-- domain record; the tiles that consume these stay client code.
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,                 -- JSON
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
);

CREATE TABLE tokens (                       -- per-writer bearer auth (ADR-0008)
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,                 -- 'pixel-9', 'sweeper', 'home-assistant'
  scope      TEXT NOT NULL CHECK (scope IN ('device','sweeper','ingest')),
  token_hash TEXT NOT NULL,                 -- sha256; plaintext shown once at mint
  created_at INTEGER NOT NULL,
  last_seen  INTEGER,
  revoked_at INTEGER
);

CREATE INDEX idx_items_version ON items(version);        -- delta pull, per table
CREATE INDEX idx_steps_version ON steps(version);
CREATE INDEX idx_items_live    ON items(stage) WHERE archived_at IS NULL;
CREATE INDEX idx_steps_item    ON steps(item_id);
CREATE INDEX idx_items_project ON items(project_id);
```

### What dissolved from the S1 model, deliberately

S1 (#94) was built against Linear's shape. Under the owned schema:
`labels: Vec<String>` → typed `size`/`energy`/`context` columns; the `extra`
passthrough bag → gone (no foreign fields to preserve); `presence: Presence`
→ explicit `archived_at` (absence-inference dissolves, ADR-0007 amendment);
`url` → the server mints its own links; `identifier` → `seq`. `priority`
survives — it is human-set intent, not a Linear-ism, and distinct from
read-time urgency. The dead-letter journal stays **client-side only**: it is
a mirror artifact, never truth.

### Sources join by role *and transport*

ADR-0002's two roles stand; context now has three lanes, distinguished by
who can hold the credential and how the data moves:

| Lane | Transport | Examples | Lifecycle |
| --- | --- | --- | --- |
| Capture | drained by the sweeper | Google Tasks, Gmail `hummingbird/capture` | create in authority, ack in source |
| Context, device-polled | client core, per-device OAuth (ADR-0005) | Google Calendar, M365 | replaced in the device mirror |
| Context, server-polled | DO cron + static keys as Worker secrets | Anthropic/OpenAI usage, GitHub repo stats, photo-site analytics, race schedules, the city waste-collection page | `context_snapshots` row replaced wholesale |
| Context, pushed | webhooks at `/api/alerts`, `ingest`-scoped tokens; or a sweeper adapter for pull-only sources | infra checks (auto-resolve), Home Assistant (dismiss/expire), Gmail `hummingbird/alert` label (dismiss), GitHub events, photo-site events | `alerts` upsert on `(source, source_key)`; leaves view by resolve / dismiss / expiry |

Rules that hold across every lane:

1. **Urgency is still computed at read time** (ADR-0002) — no stored
   importance, anywhere.
2. **A machine may raise an alert; a machine may never mint an item.** The
   gesture rule protects items, one level up: converting an alert to an item
   is one tap, a capture with `source_key` = alert id, deterministic id,
   double-tap a no-op. A server poll may machine-raise an alert on a
   threshold crossing ("80% of weekly cap") or on a **material change in
   its snapshot** (a holiday sliding trash pickup Monday → Tuesday). What
   counts as material is defined per source, at wiring time — a weekly
   "next date" rolling forward on cadence is not a change — and the alert
   must state the change itself, never just that one happened.
3. **Authorities stay authoritative:** Home Assistant decides what to send;
   the photo site decides what to push; hummingbird receives and never
   configures their rules.
4. **Provenance is the one shared shape.** Items, alerts and snapshots all
   carry `(source, source_key[, source_url])`. That is the entire common
   core — see rejected alternatives. **Every `source` string carries a `/vN`
   suffix, on all three tables** (ADR-0014): the version bump is the only
   sanctioned way to change a `source_key` recipe, and a bare name has no
   escape hatch — `anthropic-usage` → `anthropic-usage/v2` reads as a
   different service rather than a revision. The suffix also claims the
   slash, so a source naming a sub-scope folds it into the name
   (`github-hummingbird/v1`, not `github/hummingbird`).

### Standing questions read the lanes, they never add storage

Standing questions (glossary: "when is the next race," "what's this
weekend," "how long to the next vacation") are read-time consumers of the
lanes above, decided in the 2026-08-09 grilling:

- **Next race** — server-polled lane: one `context_snapshots` row per series
  (`source='f1/v1'`/`'indycar/v1'`), payload holding the upcoming schedule; "next"
  computed at read time. The same cron that refreshes the snapshot is what
  may later machine-raise a "race in 90 minutes" alert (rule 2's threshold
  carve-out) — the question answers, the alert interrupts, never one
  mechanism.
- **Weekend plans** — no new data: calendar-mirror events in the window,
  plus items *scheduled* or *due* in it. This is what forced the
  `scheduled_date`/`deadline` split — a do-date is a preference, a deadline
  has consequences, and only the latter feeds urgency (ADR-0002).
- **Vacation countdown** — device-polled lane: "next event on the dedicated
  Trips calendar." The calendar stays the authority; the question
  auto-advances when a vacation passes.
- **Which cans** — server-polled lane: a daily poll of the city's
  address-specific collection page (verified static HTML, 2026-08-09) →
  one snapshot whose payload holds each stream's cadence and next date;
  answered at read time as "which containers go out, and when." Holiday
  slides are rule 2's material-change case: the adapter judges deviation
  from cadence (the date rolling forward a week is not a change) and the
  alert names the slide — "trash: Monday → Tuesday this week."

Answers are never stored (ADR-0002 verbatim); the only persistent trace of
a standing question is its binding facts in `settings`.

Rendering is bespoke per question — a three-can graphic for waste, countdown
strings ("12 days before Monaco") for races and vacations — which is *why*
tiles are client code and a `tiles` record was rejected: there is no generic
shape to store. A pane may also render the live alerts that share its
snapshot's `source` (the waste pane shows holiday-slide text only while such
an alert is unresolved) — a deliberate use of rule 4's shared provenance
vocabulary, and a wiring constraint: a source's alerts and its snapshot must
use the same `source` string, or the pane-level join silently breaks.

### Sequencing

Alerts, snapshots, settings, token scopes and webhook endpoints are **in the
schema now** (retrofitting provenance later is what cost us this migration); their
**source wiring comes after task-parity cutover** — the move exists because
the task domain fought its authority, and alerts must not delay the cutover.
Caveat recorded: *API-account* usage has real endpoints at Anthropic and
OpenAI; *subscription-plan* usage (Claude Max, ChatGPT Plus) has no official
API — verify per source at wiring time; may be unpollable.

## Rejected alternatives

- **A generalized `Signal` table** (`source, type, importance, timestamp,
  expires_at, related_person, related_project, related_task,
  suggested_action, confidence`) — rejected on four grounds. (1) `importance`
  stores urgency at ingest, ADR-0002's founding violation; (2) one table
  forces one lifecycle onto three incompatible ones (drained / replaced /
  raise-resolve-dismiss) — an `expires_at` on a capture is scheduled data
  loss; (3) `suggested_action` + `confidence` bake an inference layer into
  the authority — machine-minting through the back door; inference is
  derived, decays, and is computed at read time by consumers; (4)
  `related_person` references a concept that does not exist in this domain,
  and relatedness flows the other way (the item records provenance when a
  gesture mints it). Its one good part — `expires_at` for self-expiring
  alerts — was adopted into `alerts`.
- **Steps as markdown in `description`** — the exact shape whose
  impossibility to edit safely under Linear triggered ADR-0008; carrying it
  into an owned schema would re-create the problem voluntarily.
- **A passthrough bag on items** — there is no foreign authority whose
  unmodelled fields need preserving; "the mirror is the export" is now
  satisfied by the schema itself.
