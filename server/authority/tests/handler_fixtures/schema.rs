//! Schema lifecycle: idempotent init, the full table set, and the additive
//! growth path (1→2, then 2→3 for the notification lane, #131).

use hummingbird_authority::{init_schema, SCHEMA_VERSION};

use crate::rig::*;

#[test]
fn init_schema_is_idempotent() {
    let sql = RusqliteSql::new(); // ran init once already
    post(&sql, r#"{"id": "a", "title": "t"}"#, 0);
    init_schema(&sql).expect("second init is a no-op");
    assert_eq!(meta_version(&sql), 1, "meta row survives re-init");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "items survive re-init");
}

#[test]
fn init_schema_creates_every_adr_0009_table() {
    let sql = RusqliteSql::new();
    let rows = sql
        .exec(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
            &[],
        )
        .unwrap();
    let names: Vec<String> = rows
        .iter()
        .map(|r| r.get("name").unwrap().as_text().unwrap().to_string())
        .collect();
    for table in [
        "meta",
        "projects",
        "routes",
        "fog",
        "items",
        "steps",
        "blocked_by",
        "alerts",
        "context_snapshots",
        "settings",
        "tokens",
        "rules",
        "push_targets",
        "deliveries",
    ] {
        assert!(names.iter().any(|n| n == table), "missing table `{table}` in {names:?}");
    }
}

/// The 1→2 growth path: a schema-1 database (S0's meta + items) is grown
/// additively — new tables appear, existing data survives, and
/// `schema_version` moves forward. No migration engine; see the
/// SCHEMA_VERSION doc for why that is a stated decision.
#[test]
fn init_schema_grows_a_schema_1_database_additively() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a", "title": "t"}"#, 0);
    sql.exec("UPDATE meta SET schema_version = 1 WHERE id = 1", &[])
        .unwrap();

    init_schema(&sql).expect("growth init succeeds");

    assert_eq!(schema_version(&sql), SCHEMA_VERSION, "schema_version moved forward");
    assert_eq!(meta_version(&sql), 1, "the workspace counter is untouched");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "existing rows survive the growth");
}

/// The frozen pre-#131 DDL, byte-for-byte the `CREATE_META`..`CREATE_TOKENS`
/// / `CREATE_INDEXES` constants `schema.rs` held before this slice landed
/// (`SCHEMA_VERSION` 2: the full ADR-0009 shape, eleven tables, five
/// indexes, no notification lane) — including their `IF NOT EXISTS` and
/// exact whitespace, so a table `init_schema` leaves untouched during
/// growth (every one of these eleven; `CREATE TABLE IF NOT EXISTS` is a
/// no-op once the table exists) still lands in `sqlite_master.sql` with the
/// identical text a fresh store would produce for it. A real v2 store is
/// frozen by definition — written and deployed before #131 existed — so
/// hardcoding its DDL here, rather than reusing anything from the current
/// `schema` module, is what makes the growth path underneath it a genuine
/// test rather than a no-op: `RusqliteSql::new()` runs the *current*
/// `init_schema` at construction, so downgrading only `meta.schema_version`
/// on top of it (the shape of the 1→2 test above) would leave `rules`,
/// `push_targets` and `deliveries` already present before growth even runs.
///
/// **Re-frozen for #153** (`items.due_date` → `items.deadline`): "frozen by
/// definition — written and deployed" is aspirational, not actual — #95's
/// human gate H3 has not fired, so *no* schema-2 store has ever really been
/// deployed. This snapshot is a test fixture standing in for one, not an
/// artifact of a real release, so it carries no more claim to `due_date`
/// than any other source file did. #153's rename is free exactly because
/// nothing is deployed (ADR-0013); leaving `due_date` here on the theory
/// that this fixture is somehow already "shipped" would smuggle a
/// migration's worth of caution into a codebase that has explicitly not
/// earned it yet, and would make this test assert a stale, wrong claim —
/// that a fresh store and a grown-from-v2 store diverge on this column —
/// which is not the invariant it exists to hold. Renaming here keeps
/// `deadline` a genuine textbook rename (no `SCHEMA_VERSION` bump, no
/// migration, per #153's acceptance criteria) rather than inventing an
/// `ALTER TABLE` this repo's ephemeral-store doctrine does not call for.
/// The day a real deploy happens, this file freezes at whatever shape is
/// live then — not before.
const V2_TABLES: &[&str] = &[
    "\
CREATE TABLE IF NOT EXISTS meta (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  version        INTEGER NOT NULL,
  schema_version INTEGER NOT NULL
)",
    "\
CREATE TABLE IF NOT EXISTS projects (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)",
    "\
CREATE TABLE IF NOT EXISTS routes (
  project_id  TEXT PRIMARY KEY REFERENCES projects(id),
  destination TEXT,
  notes       TEXT,
  updated_at  INTEGER NOT NULL,
  version     INTEGER NOT NULL
)",
    "\
CREATE TABLE IF NOT EXISTS fog (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  question    TEXT NOT NULL,
  position    INTEGER NOT NULL,
  resolved_at INTEGER,
  version     INTEGER NOT NULL
)",
    "\
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
)",
    "\
CREATE TABLE IF NOT EXISTS steps (
  id         TEXT PRIMARY KEY,
  item_id    TEXT NOT NULL REFERENCES items(id),
  body       TEXT NOT NULL,
  done       INTEGER NOT NULL DEFAULT 0,
  position   INTEGER NOT NULL,
  deleted_at INTEGER,
  version    INTEGER NOT NULL
)",
    "\
CREATE TABLE IF NOT EXISTS blocked_by (
  item_id    TEXT NOT NULL REFERENCES items(id),
  blocker_id TEXT NOT NULL REFERENCES items(id),
  version    INTEGER NOT NULL,
  removed_at INTEGER,
  PRIMARY KEY (item_id, blocker_id),
  CHECK (item_id <> blocker_id)
)",
    "\
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
)",
    "\
CREATE TABLE IF NOT EXISTS context_snapshots (
  source     TEXT NOT NULL,
  key        TEXT NOT NULL,
  payload    TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  version    INTEGER NOT NULL,
  PRIMARY KEY (source, key)
)",
    "\
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
)",
    "\
CREATE TABLE IF NOT EXISTS tokens (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  scope      TEXT NOT NULL CHECK (scope IN ('device','sweeper','ingest')),
  token_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen  INTEGER,
  revoked_at INTEGER
)",
];

const V2_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_items_version ON items(version)",
    "CREATE INDEX IF NOT EXISTS idx_steps_version ON steps(version)",
    "CREATE INDEX IF NOT EXISTS idx_items_live    ON items(stage) WHERE archived_at IS NULL",
    "CREATE INDEX IF NOT EXISTS idx_steps_item    ON steps(item_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_project ON items(project_id)",
];

/// A store built straight from the frozen v2 DDL above — never touched by
/// the current `init_schema` — with `meta` seeded exactly as a real v2
/// store's would be.
fn v2_store() -> RusqliteSql {
    let sql = RusqliteSql {
        conn: rusqlite::Connection::open_in_memory().expect("in-memory sqlite opens"),
    };
    for ddl in V2_TABLES.iter().chain(V2_INDEXES.iter()) {
        sql.exec(ddl, &[]).expect("v2 DDL applies");
    }
    sql.exec(
        "INSERT INTO meta (id, version, schema_version) VALUES (1, 0, 2)",
        &[],
    )
    .expect("v2 meta row seeds");
    sql
}

/// The 2→3 growth path (#131), against a genuine v2 database (never
/// initialized at the current `SCHEMA_VERSION`): `rules`, `push_targets`
/// and `deliveries` appear additively, and the grown schema is
/// byte-for-byte identical to a fresh store's — not just the same table
/// names, but the same `sqlite_master.sql` for every table and index
/// (which is what actually pins `idx_rules_version` existing, not merely
/// the table set).
#[test]
fn init_schema_grows_a_schema_2_database_additively() {
    let migrated = v2_store();
    assert_eq!(schema_version(&migrated), 2, "starts genuinely at v2");
    assert!(
        !table_names(&migrated).contains(&"rules".to_string()),
        "the v2 fixture must not already carry the notification lane"
    );

    init_schema(&migrated).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    for table in ["rules", "push_targets", "deliveries"] {
        assert!(
            table_names(&migrated).contains(&table.to_string()),
            "migrated store missing `{table}`",
        );
    }

    let fresh = RusqliteSql::new();
    assert_eq!(
        table_names(&migrated),
        table_names(&fresh),
        "a migrated v2 store and a fresh store end up with identical table sets",
    );
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v2 store and a fresh store end up with byte-identical DDL, \
         including every index (idx_rules_version among them)",
    );
}

fn schema_version(sql: &dyn Sql) -> i64 {
    sql.exec("SELECT schema_version FROM meta WHERE id = 1", &[])
        .unwrap()[0]
        .get("schema_version")
        .unwrap()
        .as_i64()
        .unwrap()
}

fn table_names(sql: &dyn Sql) -> Vec<String> {
    sql.exec(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| r.get("name").unwrap().as_text().unwrap().to_string())
    .collect()
}

/// Every table's and index's own `CREATE` statement, name-ordered — the
/// full schema shape, not just which objects exist.
fn schema_ddl(sql: &dyn Sql) -> Vec<(String, String)> {
    sql.exec(
        "SELECT name, sql FROM sqlite_master \
         WHERE type IN ('table', 'index') AND sql IS NOT NULL \
         ORDER BY name",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| {
        (
            r.get("name").unwrap().as_text().unwrap().to_string(),
            r.get("sql").unwrap().as_text().unwrap().to_string(),
        )
    })
    .collect()
}
