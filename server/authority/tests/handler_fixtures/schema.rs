//! Schema lifecycle: idempotent init, the full table set, and the additive
//! growth path (1→2, then 2→3 for the notification lane, #131, then 3→4 for
//! ADR-0015's `alerts.subject_key`).

use hummingbird_authority::{init_schema, SqlValue, SCHEMA_VERSION};

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
///
/// **Re-frozen again for #145** (`tokens` gains `source`): same doctrine.
/// `CREATE TABLE IF NOT EXISTS` is a no-op once a table exists, so growing
/// a *genuinely* column-missing v2 `tokens` table forward would never pick
/// up `source` — that would only be the right growth story for a shape
/// that was ever really deployed. Nothing was, so the fixture is simply
/// re-frozen to already carry the column, exactly as `deadline` was.
///
/// **Deliberately *not* re-frozen for ADR-0015's `alerts.subject_key`.**
/// That doctrine applies where there is no migration to exercise; 3→4 has
/// one (`init_schema`'s `ALTER TABLE`), so leaving this `alerts` genuinely
/// column-less makes the 2→3 test above pass through it too — a second,
/// free covering of the growth path. Re-freezing here would quietly delete
/// that coverage.
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
  source     TEXT,
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

/// What 2→3 added (#131): the notification lane, frozen here exactly as
/// `schema.rs` held it before ADR-0015 — three tables and one index. A real
/// v3 store is a v2 store plus these, which is what [`v3_store`] builds.
const V3_ADDED_TABLES: &[&str] = &[
    "\
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
)",
    "\
CREATE TABLE IF NOT EXISTS push_targets (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  platform   TEXT NOT NULL CHECK (platform IN ('android','ios')),
  fcm_token  TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen  INTEGER,
  revoked_at INTEGER
)",
    "\
CREATE TABLE IF NOT EXISTS deliveries (
  id         TEXT PRIMARY KEY,
  alert_id   TEXT NOT NULL REFERENCES alerts(id),
  rule_id    TEXT NOT NULL REFERENCES rules(id),
  generation INTEGER NOT NULL,
  severity   TEXT NOT NULL,
  tier       TEXT NOT NULL,
  sent_at    INTEGER NOT NULL,
  UNIQUE(alert_id, rule_id, generation, severity)
)",
];

const V3_ADDED_INDEXES: &[&str] =
    &["CREATE INDEX IF NOT EXISTS idx_rules_version ON rules(version)"];

/// A store built straight from the frozen v2 + v3 DDL — never touched by the
/// current `init_schema`, so its `alerts` table genuinely lacks
/// `subject_key`, which is the whole point. Downgrading only
/// `meta.schema_version` on top of a current store would not do: the column
/// would already be there and the growth path underneath would be a no-op.
fn v3_store() -> RusqliteSql {
    let sql = RusqliteSql {
        conn: rusqlite::Connection::open_in_memory().expect("in-memory sqlite opens"),
    };
    for ddl in V2_TABLES
        .iter()
        .chain(V3_ADDED_TABLES.iter())
        .chain(V2_INDEXES.iter())
        .chain(V3_ADDED_INDEXES.iter())
    {
        sql.exec(ddl, &[]).expect("v3 DDL applies");
    }
    sql.exec(
        "INSERT INTO meta (id, version, schema_version) VALUES (1, 0, 3)",
        &[],
    )
    .expect("v3 meta row seeds");
    sql
}

/// The 3→4 growth path (ADR-0015), against a genuine v3 database: the first
/// growth that adds a **column to an existing table** rather than a whole
/// table. `CREATE TABLE IF NOT EXISTS alerts (…)` is a silent no-op on a
/// store that already has an `alerts` table, so without the `ALTER TABLE` in
/// `init_schema` the column would simply never appear while
/// `schema_version` marched to 4 regardless — nothing would fail loudly.
/// That is why this test asserts the column's actual presence and not only
/// the DDL text.
#[test]
fn init_schema_grows_a_schema_3_database_additively() {
    let migrated = v3_store();
    assert_eq!(schema_version(&migrated), 3, "starts genuinely at v3");
    assert!(
        !column_names(&migrated, "alerts").contains(&"subject_key".to_string()),
        "the v3 fixture must not already carry subject_key",
    );
    // A row written before the growth, to prove the ALTER keeps data.
    migrated
        .exec(
            "INSERT INTO alerts (id, source, source_key, title, raised_at, version) \
             VALUES ('a', 'hc/v1', 'k', 't', 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(
        column_names(&migrated, "alerts").contains(&"subject_key".to_string()),
        "the migrated store actually has the column, not just a bumped version",
    );
    let rows = migrated.exec("SELECT id, subject_key FROM alerts", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");
    assert_eq!(
        rows[0].get("subject_key"),
        Some(&SqlValue::Null),
        "an alert minted before ADR-0015 names no subject",
    );

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v3 store and a fresh store end up with byte-identical DDL — which is \
         why CREATE_ALERTS declares subject_key last and inline, matching what ALTER TABLE \
         ADD COLUMN splices in",
    );
}

/// A genuine v4 store: the frozen v2+v3 DDL plus 3→4's own `ALTER`, which
/// is how every real v4 store came to be. Built this way rather than by
/// freezing a fourth DDL block because that is exactly the text a v4
/// database holds — and, crucially, its `items` still genuinely lacks
/// `agent`, which is what the 4→5 growth has to find.
///
/// This is the first fixture standing in for a shape that was **really
/// deployed** (#237, 2026-08-10), so the re-freeze doctrine the header
/// describes no longer applies to it: there is a live store out there in
/// this shape and the migration is the only thing that grows it.
fn v4_store() -> RusqliteSql {
    let sql = v3_store();
    sql.exec("ALTER TABLE alerts ADD COLUMN subject_key TEXT", &[])
        .expect("3→4's own ALTER applies");
    sql.exec("UPDATE meta SET schema_version = 4 WHERE id = 1", &[])
        .expect("v4 meta row seeds");
    sql
}

/// The 4→5 growth path (#115/#291): `items.agent`, the second column ever
/// added to an existing table, and the first added after a production
/// deploy.
///
/// The DDL assertion is the one with a wrong answer available, and it is
/// **not** the same wrong answer 3→4 had. `ALTER TABLE … ADD COLUMN`
/// splices its text at the start of the table-constraint list, falling back
/// to the closing paren when a table has no constraints at all — so
/// `alerts` (which ends in `UNIQUE(…)`) takes the column snug against
/// `version`'s line, while `items` (which has no table constraint) takes it
/// after the newline. Formatting `CREATE_ITEMS` the way `CREATE_ALERTS` is
/// formatted fails here on that single newline, which is how this was
/// found.
#[test]
fn init_schema_grows_a_schema_4_database_additively() {
    let migrated = v4_store();
    assert_eq!(schema_version(&migrated), 4, "starts genuinely at v4");
    assert!(
        !column_names(&migrated, "items").contains(&"agent".to_string()),
        "the v4 fixture must not already carry agent",
    );
    // A row written before the growth, to prove the ALTER keeps data.
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, priority, created_at, updated_at, version) \
             VALUES ('i', 'compare three insurance quotes', 'ready', 0, 1000, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(
        column_names(&migrated, "items").contains(&"agent".to_string()),
        "the migrated store actually has the column, not just a bumped version",
    );
    let rows = migrated.exec("SELECT id, agent FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");
    assert_eq!(
        rows[0].get("agent"),
        Some(&SqlValue::Integer(0)),
        "an item minted before the delegation axis is the human's — the column's \
         NOT NULL DEFAULT 0 is what makes the ALTER legal on a non-empty table at all",
    );

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v4 store and a fresh store end up with byte-identical DDL — which is \
         why CREATE_ITEMS declares agent after the newline, before the closing paren, \
         and NOT inline the way CREATE_ALERTS declares subject_key",
    );
}

/// Running `init_schema` twice over a grown store must not attempt either
/// `ALTER` again — a duplicate column is a hard SQLite error, and
/// `init_schema` runs on every Durable Object construction.
#[test]
fn the_column_migration_is_idempotent() {
    let migrated = v3_store();
    init_schema(&migrated).expect("first growth succeeds");
    init_schema(&migrated).expect("second init is a no-op, not a duplicate-column error");
    assert_eq!(
        column_names(&migrated, "alerts")
            .iter()
            .filter(|name| *name == "subject_key")
            .count(),
        1,
        "exactly one subject_key column",
    );
    assert_eq!(
        column_names(&migrated, "items")
            .iter()
            .filter(|name| *name == "agent")
            .count(),
        1,
        "exactly one agent column",
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

/// One table's column names, in declaration order. `PRAGMA table_info` is
/// fine *here* — the test rig is rusqlite only; `schema.rs` itself reads
/// `sqlite_master` instead, because the Durable Object allows far fewer
/// pragmas than a local SQLite does.
fn column_names(sql: &dyn Sql, table: &str) -> Vec<String> {
    sql.exec(&format!("PRAGMA table_info({table})"), &[])
        .unwrap()
        .iter()
        .map(|r| r.get("name").unwrap().as_text().unwrap().to_string())
        .collect()
}
