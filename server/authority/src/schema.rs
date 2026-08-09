//! The S0 slice of the owned schema: `meta` + `items`, verbatim from
//! ADR-0009 with two deliberate deviations, each marked below. #114 grows
//! this to the full DDL under a `schema_version` bump.

use crate::sql::{Sql, SqlError, SqlValue};

/// Bumped when the DDL changes shape; stored in `meta.schema_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// meta: the workspace version counter (one row), bumped by every write.
/// Every mutated row stamps its `version` from this counter; the delta pull
/// is "WHERE version > ?" per table. Rows are never deleted, only flagged.
pub const CREATE_META: &str = "\
CREATE TABLE IF NOT EXISTS meta (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  version        INTEGER NOT NULL,
  schema_version INTEGER NOT NULL
)";

/// ADR-0009's `items`, minus `REFERENCES projects(id)` on `project_id` —
/// the `projects` table arrives with #114, and S0 must not carry a dangling
/// FK.
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
  project_id  TEXT,
  project_pos INTEGER,
  due_date    TEXT,
  scheduled_date TEXT,
  source      TEXT,
  source_key  TEXT,
  source_url  TEXT,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)";

/// The delta pull's index. The other ADR-0009 indexes serve tables and
/// queries S0 doesn't have; they arrive with #114.
pub const CREATE_IDX_ITEMS_VERSION: &str =
    "CREATE INDEX IF NOT EXISTS idx_items_version ON items(version)";

/// Idempotent: safe to run on every Durable Object construction.
pub fn init_schema(sql: &dyn Sql) -> Result<(), SqlError> {
    sql.exec(CREATE_META, &[])?;
    sql.exec(CREATE_ITEMS, &[])?;
    sql.exec(CREATE_IDX_ITEMS_VERSION, &[])?;
    sql.exec(
        "INSERT OR IGNORE INTO meta (id, version, schema_version) VALUES (1, 0, ?)",
        &[SqlValue::Integer(SCHEMA_VERSION)],
    )?;
    Ok(())
}
