//! The full owned schema, verbatim from ADR-0009 as amended 2026-08-09
//! (`scheduled_date`, `settings`), plus the notification lane's three
//! tables (ADR-0012, amended by ADR-0013) landed here in their own slice
//! (#131). Fourteen tables and six indexes.

use crate::sql::{Sql, SqlError, SqlValue};

/// Bumped when the DDL changes shape; stored in `meta.schema_version`.
/// 1 was the S0 slice (`meta` + `items`, no FK); 2 was the full ADR-0009
/// DDL; 3 adds the notification lane (`rules`, `push_targets`,
/// `deliveries`, #131). The bump is additive-only — every pre-3 consumer is
/// ephemeral (in-memory fixtures, a `wrangler dev` state dir the smoke
/// script wipes), so there is no migration engine; the first breaking DDL
/// change after a production deploy earns one.
pub const SCHEMA_VERSION: i64 = 3;

/// meta: the workspace version counter (one row), bumped by every write.
/// Every mutated row stamps its `version` from this counter; the delta pull
/// is "WHERE version > ?" per table. Rows are never deleted, only flagged.
pub const CREATE_META: &str = "\
CREATE TABLE IF NOT EXISTS meta (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  version        INTEGER NOT NULL,
  schema_version INTEGER NOT NULL
)";

pub const CREATE_PROJECTS: &str = "\
CREATE TABLE IF NOT EXISTS projects (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)";

/// Route: 1:1 with project, separate table because /to-actions owns it
/// (glossary: Destination, Fog, Notes, ordered actions).
pub const CREATE_ROUTES: &str = "\
CREATE TABLE IF NOT EXISTS routes (
  project_id  TEXT PRIMARY KEY REFERENCES projects(id),
  destination TEXT,
  notes       TEXT,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)";

/// Fog: segments not yet definable as actions, each with its open question.
pub const CREATE_FOG: &str = "\
CREATE TABLE IF NOT EXISTS fog (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  question    TEXT NOT NULL,
  position    INTEGER NOT NULL,
  resolved_at INTEGER,
  version     INTEGER NOT NULL
)";

/// A database created at schema 1 keeps its FK-less `items` (IF NOT EXISTS
/// skips it) — acceptable because every pre-2 consumer is ephemeral; see
/// [`SCHEMA_VERSION`]. FKs are documentation here regardless: enforcement
/// is off by default in SQLite, and handlers validate referents explicitly.
pub const CREATE_ITEMS: &str = "\
CREATE TABLE IF NOT EXISTS items (
  id          TEXT PRIMARY KEY,
  seq         INTEGER UNIQUE,
  title       TEXT NOT NULL CHECK (length(title) > 0),
  description TEXT,
  stage       TEXT NOT NULL CHECK (stage IN
                ('triage','grilling','ready','in_progress','blocked','done')),
  size        TEXT CHECK (size IN ('quick','short','deep')),
  energy      TEXT CHECK (energy IN ('low','medium','high')),
  context     TEXT,
  priority    INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
  project_id  TEXT REFERENCES projects(id),
  project_pos INTEGER,
  deadline    TEXT,
  scheduled_date TEXT,
  source      TEXT,
  source_key  TEXT,
  source_url  TEXT,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)";

pub const CREATE_STEPS: &str = "\
CREATE TABLE IF NOT EXISTS steps (
  id         TEXT PRIMARY KEY,
  item_id    TEXT NOT NULL REFERENCES items(id),
  body       TEXT NOT NULL,
  done       INTEGER NOT NULL DEFAULT 0,
  position   INTEGER NOT NULL,
  deleted_at INTEGER,
  version    INTEGER NOT NULL
)";

/// Native sequencing edges.
pub const CREATE_BLOCKED_BY: &str = "\
CREATE TABLE IF NOT EXISTS blocked_by (
  item_id    TEXT NOT NULL REFERENCES items(id),
  blocker_id TEXT NOT NULL REFERENCES items(id),
  version    INTEGER NOT NULL,
  removed_at INTEGER,
  PRIMARY KEY (item_id, blocker_id),
  CHECK (item_id <> blocker_id)
)";

/// Pushed context: discrete events raised at the app from outside.
pub const CREATE_ALERTS: &str = "\
CREATE TABLE IF NOT EXISTS alerts (
  id           TEXT PRIMARY KEY,
  source       TEXT NOT NULL,
  source_key   TEXT NOT NULL,
  title        TEXT NOT NULL,
  body         TEXT,
  url          TEXT,
  severity     TEXT,
  raised_at    INTEGER NOT NULL,
  resolved_at  INTEGER,
  dismissed_at INTEGER,
  expires_at   INTEGER,
  version      INTEGER NOT NULL,
  UNIQUE(source, source_key)
)";

/// Server-polled context: gauges replaced wholesale each poll, never drained.
pub const CREATE_CONTEXT_SNAPSHOTS: &str = "\
CREATE TABLE IF NOT EXISTS context_snapshots (
  source     TEXT NOT NULL,
  key        TEXT NOT NULL,
  payload    TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  version    INTEGER NOT NULL,
  PRIMARY KEY (source, key)
)";

/// Workspace preferences: small cross-device binding facts, synced through
/// the normal delta pull. Machinery like meta and tokens, not a domain
/// record; the tiles that consume these stay client code.
pub const CREATE_SETTINGS: &str = "\
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
)";

/// Per-writer bearer auth (ADR-0008). Never synced: no `version` column —
/// tokens are outside the delta contract by construction.
pub const CREATE_TOKENS: &str = "\
CREATE TABLE IF NOT EXISTS tokens (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  scope      TEXT NOT NULL CHECK (scope IN ('device','sweeper','ingest')),
  token_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen  INTEGER,
  revoked_at INTEGER
)";

/// The notification lane (ADR-0012, amended by ADR-0013). `event_kind` is
/// nullable with **no `CHECK`** — an open registry key, `NULL` = any kind;
/// a `CHECK` here would freeze the registry in the database, which is
/// exactly what #133 avoids by keeping it in code. `conditions` is JSON
/// (`[{field, op, value, negate}]`, the `negate` member new in 0013) with no
/// column-level shape enforcement — the typed catalogue lives in `domain`,
/// validated at save by the rule engine (#133), not by SQLite. `severity`
/// stays `NOT NULL` even though it is unused for `mints: false` kinds.
pub const CREATE_RULES: &str = "\
CREATE TABLE IF NOT EXISTS rules (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  event_kind TEXT,
  conditions TEXT NOT NULL,
  severity   TEXT NOT NULL,
  tier       TEXT NOT NULL CHECK (tier IN ('urgent','normal')),
  enabled    INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
)";

/// FCM registrations (ADR-0012): the notification sibling of `tokens`,
/// individually revocable. No `version` — server-side machinery outside the
/// delta-pull contract, like `tokens`; the HTTP surface that creates and
/// revokes rows here is #139.
pub const CREATE_PUSH_TARGETS: &str = "\
CREATE TABLE IF NOT EXISTS push_targets (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  platform   TEXT NOT NULL CHECK (platform IN ('android','ios')),
  fcm_token  TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen  INTEGER,
  revoked_at INTEGER
)";

/// The delivery log (ADR-0012, amended by ADR-0014): the durable memory of
/// what rang. `UNIQUE(alert_id, rule_id, generation, severity)` is the
/// dedupe key #139's transitions-only dedupe reads — an identical re-raise
/// of a live alert must not insert a second row for the *same* rule.
/// `rule_id` is in the key deliberately (ADR-0014): a delivery is one
/// rule's ring, not the alert's — N matching rules produce N delivery rows
/// even when they agree on severity, so `rule_id` must not collapse them.
/// No `version`: an append-only log, outside the delta-pull contract like
/// `push_targets`.
pub const CREATE_DELIVERIES: &str = "\
CREATE TABLE IF NOT EXISTS deliveries (
  id         TEXT PRIMARY KEY,
  alert_id   TEXT NOT NULL REFERENCES alerts(id),
  rule_id    TEXT NOT NULL REFERENCES rules(id),
  generation INTEGER NOT NULL,
  severity   TEXT NOT NULL,
  tier       TEXT NOT NULL,
  sent_at    INTEGER NOT NULL,
  UNIQUE(alert_id, rule_id, generation, severity)
)";

const CREATE_INDEXES: [&str; 6] = [
    "CREATE INDEX IF NOT EXISTS idx_items_version ON items(version)",
    "CREATE INDEX IF NOT EXISTS idx_steps_version ON steps(version)",
    "CREATE INDEX IF NOT EXISTS idx_items_live    ON items(stage) WHERE archived_at IS NULL",
    "CREATE INDEX IF NOT EXISTS idx_steps_item    ON steps(item_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_project ON items(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_rules_version ON rules(version)",
];

/// Every table, parents before children (routes/fog reference projects,
/// steps/blocked_by reference items, deliveries references alerts/rules).
const CREATE_TABLES: [&str; 14] = [
    CREATE_META,
    CREATE_PROJECTS,
    CREATE_ROUTES,
    CREATE_FOG,
    CREATE_ITEMS,
    CREATE_STEPS,
    CREATE_BLOCKED_BY,
    CREATE_ALERTS,
    CREATE_CONTEXT_SNAPSHOTS,
    CREATE_SETTINGS,
    CREATE_TOKENS,
    CREATE_RULES,
    CREATE_PUSH_TARGETS,
    CREATE_DELIVERIES,
];

/// Idempotent: safe to run on every Durable Object construction. A schema-1
/// database is grown additively — the new tables are created, `items` stays
/// as-is, and `schema_version` moves forward.
pub fn init_schema(sql: &dyn Sql) -> Result<(), SqlError> {
    for ddl in CREATE_TABLES.iter().chain(CREATE_INDEXES.iter()) {
        sql.exec(ddl, &[])?;
    }
    sql.exec(
        "INSERT OR IGNORE INTO meta (id, version, schema_version) VALUES (1, 0, ?)",
        &[SqlValue::Integer(SCHEMA_VERSION)],
    )?;
    sql.exec(
        "UPDATE meta SET schema_version = ? WHERE id = 1 AND schema_version < ?",
        &[
            SqlValue::Integer(SCHEMA_VERSION),
            SqlValue::Integer(SCHEMA_VERSION),
        ],
    )?;
    Ok(())
}
