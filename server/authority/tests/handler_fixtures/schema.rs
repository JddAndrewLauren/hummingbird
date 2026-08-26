//! Schema lifecycle: idempotent init, the full table set, and the additive
//! growth path (1→2, then 2→3 for the notification lane, #131, then 3→4 for
//! ADR-0015's `alerts.subject_key`, then 4→5 for `items.agent`, then 5→6
//! for `grills`, #353) — then 6→7, the one growth that is not additive
//! at all: the `short` → `normal` size rename (#446, ADR-0024), which
//! rebuilds `items` — and then 7→8, back to additive: `projects.github_repo`
//! and `projects.default_context` (#625, ADR-0030) — then 8→9, additive
//! again: `project_links` (#626, ADR-0030 decision 4) — then 9→10,
//! `rules.deleted_at` (#727) — then 10→11, additive again: a
//! version-leading index on every synced table (#289) — then 11→12, the
//! first growth that is a pure `DROP`: the five bare version indexes #289
//! left standing become prefix-subsumed once their `_version_id` compound
//! sibling exists (#757).

use hummingbird_authority::{init_schema, SqlValue, SCHEMA_VERSION};

use crate::rig::*;

#[test]
fn init_schema_is_idempotent() {
    let sql = RusqliteSql::new(); // ran init once already
    post(&sql, r#"{"id": "a", "title": "t"}"#, 0);
    init_schema(&sql, 0).expect("second init is a no-op");
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
        "project_links",
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
        "grills",
    ] {
        assert!(names.iter().any(|n| n == table), "missing table `{table}` in {names:?}");
    }
}

/// Pins `project_links`' surviving index (#626) by name. The migrated-vs-
/// fresh `sqlite_master.sql` equality assertions below structurally cannot
/// see a dropped index — deleting one from `CREATE_INDEXES` removes it from
/// BOTH sides of the comparison, and equality still holds — so existence has
/// to be asserted somewhere, once, by name. `idx_project_links_version` is
/// no longer one of them: #757 dropped it as prefix-subsumed by
/// `idx_project_links_version_id`, and its absence is asserted alongside
/// that growth path rather than here.
#[test]
fn init_schema_creates_the_project_links_indexes() {
    let sql = RusqliteSql::new();
    let rows = sql
        .exec(
            "SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name",
            &[],
        )
        .unwrap();
    let names: Vec<String> = rows
        .iter()
        .map(|r| r.get("name").unwrap().as_text().unwrap().to_string())
        .collect();
    for index in ["idx_project_links_version_id", "idx_project_links_project"] {
        assert!(names.iter().any(|n| n == index), "missing index `{index}` in {names:?}");
    }
    assert!(
        !names.iter().any(|n| n == "idx_project_links_version"),
        "idx_project_links_version should be gone (#757)",
    );
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

    init_schema(&sql, 0).expect("growth init succeeds");

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
/// names, but the same `sqlite_master.sql` for every table and index —
/// which is what actually pins the index set, not merely the table set.
/// (`idx_rules_version` appears transiently from the v3 fixture's own frozen
/// DDL below, then #757's growth step drops it same as on a fresh store.)
#[test]
fn init_schema_grows_a_schema_2_database_additively() {
    let migrated = v2_store();
    assert_eq!(schema_version(&migrated), 2, "starts genuinely at v2");
    assert!(
        !table_names(&migrated).contains(&"rules".to_string()),
        "the v2 fixture must not already carry the notification lane"
    );

    init_schema(&migrated, 0).expect("growth init succeeds");

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
         including every surviving index",
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

    init_schema(&migrated, 0).expect("growth init succeeds");

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

    init_schema(&migrated, 0).expect("growth init succeeds");

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

/// A genuine v5 store: the frozen v2+v3 DDL, 3→4's `ALTER`, and 4→5's
/// `ALTER`, which is how every real v5 store came to be — `hummingbird-
/// authority` has been live at this shape since #237/#291.
fn v5_store() -> RusqliteSql {
    let sql = v4_store();
    sql.exec(
        "ALTER TABLE items ADD COLUMN agent INTEGER NOT NULL DEFAULT 0",
        &[],
    )
    .expect("4→5's own ALTER applies");
    sql.exec("UPDATE meta SET schema_version = 5 WHERE id = 1", &[])
        .expect("v5 meta row seeds");
    sql
}

/// The 5→6 growth path (#353): `grills`, back to the 1→2 / 2→3 shape — a
/// purely additive new table, not another [`add_missing_columns`] arm.
/// `CREATE TABLE IF NOT EXISTS` grows a v5 store for free, so this is the
/// same style of test as `init_schema_grows_a_schema_2_database_additively`
/// rather than the 3→4 / 4→5 column-presence style.
#[test]
fn init_schema_grows_a_schema_5_database_additively() {
    let migrated = v5_store();
    assert_eq!(schema_version(&migrated), 5, "starts genuinely at v5");
    assert!(
        !table_names(&migrated).contains(&"grills".to_string()),
        "the v5 fixture must not already carry grills"
    );
    // A row written before the growth, to prove the table addition never
    // touches anything pre-existing.
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, priority, created_at, updated_at, version, agent) \
             VALUES ('i', 'compare three insurance quotes', 'ready', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(
        table_names(&migrated).contains(&"grills".to_string()),
        "migrated store missing `grills`",
    );
    let rows = migrated.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");

    let fresh = RusqliteSql::new();
    assert_eq!(
        table_names(&migrated),
        table_names(&fresh),
        "a migrated v5 store and a fresh store end up with identical table sets",
    );
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v5 store and a fresh store end up with byte-identical DDL, \
         including idx_grills_item (idx_grills_version was also new here, but #757 \
         drops it from both sides alike)",
    );
}

/// A genuine v6 store: a v5 one plus `grills` and its two indexes, which is
/// how every real v6 store came to be. Its `items` still says `'short'` —
/// that is the shape 6→7 has to find and rewrite.
fn v6_store() -> RusqliteSql {
    let sql = v5_store();
    sql.exec(
        "\
CREATE TABLE IF NOT EXISTS grills (
  id              TEXT PRIMARY KEY,
  item_id         TEXT NOT NULL REFERENCES items(id),
  transcript      TEXT NOT NULL,
  summary         TEXT NOT NULL,
  verdict         TEXT NOT NULL CHECK (verdict IN ('resolved','fog_remains')),
  model_proposal  TEXT NOT NULL,
  applied_patch   TEXT NOT NULL,
  resulting_stage TEXT NOT NULL CHECK (resulting_stage IN
                     ('triage','grilling','ready','in_progress','blocked','done')),
  completed_at    INTEGER NOT NULL,
  version         INTEGER NOT NULL
)",
        &[],
    )
    .expect("5→6's own table applies");
    for index in [
        "CREATE INDEX IF NOT EXISTS idx_grills_version ON grills(version)",
        "CREATE INDEX IF NOT EXISTS idx_grills_item    ON grills(item_id)",
    ] {
        sql.exec(index, &[]).expect("5→6's own indexes apply");
    }
    sql.exec("UPDATE meta SET schema_version = 6 WHERE id = 1", &[])
        .expect("v6 meta row seeds");
    sql
}

/// The 6→7 rewrite (#446, ADR-0024): `short` → `normal` inside `items`'
/// `CHECK` constraint and in the rows already stored under it. The first
/// growth that is not additive — SQLite cannot alter a constraint, so
/// `init_schema` rebuilds the table.
///
/// Four things have a wrong answer available, and this test is here for all
/// four:
///
/// 1. **The stored row.** A `'short'` item must read `'normal'` afterwards;
///    a `'quick'` one must be left exactly alone.
/// 2. **The children.** `steps` and `blocked_by` hold foreign keys onto
///    `items(id)`. The rebuild drops the parent table, so the rows are one
///    mis-ordered statement away from disappearing — and the clauses one
///    stray `RENAME` away from pointing at a scratch table.
/// 3. **The DDL.** The rebuilt table must be **byte-identical** to a fresh
///    one, indexes included. This is what rules out the textbook
///    `CREATE … / DROP / RENAME`: `RENAME TO` rewrites the stored text to
///    `CREATE TABLE "items"`, quoted and without `IF NOT EXISTS`, which
///    passes a `CHECK`-constraint assertion and fails this one.
/// 4. **The second boot.** `init_schema` runs on every Durable Object
///    construction, so the rebuild has to see its own output and decline.
#[test]
fn init_schema_rewrites_a_schema_6_database_size_vocabulary() {
    let migrated = v6_store();
    assert_eq!(schema_version(&migrated), 6, "starts genuinely at v6");
    assert!(
        items_ddl(&migrated).contains("'short'"),
        "the v6 fixture must still carry the old size vocabulary",
    );

    migrated
        .exec(
            "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
             VALUES ('i', 'compare three insurance quotes', 'ready', 'short', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .unwrap();
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
             VALUES ('q', 'take the bins out', 'ready', 'quick', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .unwrap();
    migrated
        .exec(
            "INSERT INTO steps (id, item_id, body, done, position, version) \
             VALUES ('s', 'i', 'open the comparison site', 0, 0, 1)",
            &[],
        )
        .unwrap();
    migrated
        .exec(
            "INSERT INTO blocked_by (item_id, blocker_id, version) VALUES ('i', 'q', 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("the rebuild succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    let sizes = migrated
        .exec("SELECT id, size FROM items ORDER BY id", &[])
        .unwrap();
    assert_eq!(sizes.len(), 2, "both pre-rebuild rows survive");
    assert_eq!(
        sizes[0].get("size"),
        Some(&SqlValue::Text("normal".to_string())),
        "the 'short' row reads 'normal' — the rename reached the data, not just the constraint",
    );
    assert_eq!(
        sizes[1].get("size"),
        Some(&SqlValue::Text("quick".to_string())),
        "an untouched value stays untouched",
    );

    let steps = migrated.exec("SELECT id, item_id FROM steps", &[]).unwrap();
    assert_eq!(steps.len(), 1, "the child step survives the parent's rebuild");
    assert_eq!(steps[0].get("item_id"), Some(&SqlValue::Text("i".to_string())));
    let edges = migrated
        .exec("SELECT item_id, blocker_id FROM blocked_by", &[])
        .unwrap();
    assert_eq!(edges.len(), 1, "the blocked_by edge survives the parent's rebuild");
    assert!(
        schema_ddl(&migrated)
            .iter()
            .any(|(name, ddl)| name == "steps" && ddl.contains("REFERENCES items(id)")),
        "the children still reference `items` by its own name — nothing was renamed out from \
         under them",
    );

    // The new constraint actually binds, in both directions.
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
             VALUES ('n', 'a normal-sized thing', 'ready', 'normal', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .expect("'normal' is accepted by the rebuilt CHECK");
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
             VALUES ('o', 'an old-vocabulary thing', 'ready', 'short', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .expect_err("'short' is no longer a legal size");

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a rebuilt v6 store and a fresh store hold byte-identical DDL — which is why the \
         rebuild recreates `items` from CREATE_ITEMS rather than renaming a temp table into \
         place, and why the index loop runs after it",
    );
}

/// The rebuild sees its own output and declines — `init_schema` runs on
/// every Durable Object construction, and a second rebuild would copy the
/// table for nothing at best.
#[test]
fn the_size_rebuild_is_idempotent() {
    let migrated = v6_store();
    init_schema(&migrated, 0).expect("first rebuild succeeds");
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
             VALUES ('i', 'written between the two boots', 'ready', 'normal', 0, 1000, 1000, 1, 0)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("second init is a no-op, not a second rebuild");

    let rows = migrated.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the row written after the rebuild survives the next boot");
    assert!(
        migrated
            .exec(
                "SELECT name FROM sqlite_master WHERE name = 'items_size_rebuild'",
                &[],
            )
            .unwrap()
            .is_empty(),
        "the scratch table is not left behind",
    );
}

/// Seed a v6 store with the two items, the step and the edge the rebuild
/// tests all need, so the interruption tests below differ only in *where*
/// they cut the sequence.
fn v6_store_with_rows() -> RusqliteSql {
    let sql = v6_store();
    sql.exec(
        "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
         VALUES ('i', 'compare three insurance quotes', 'ready', 'short', 0, 1000, 1000, 1, 0)",
        &[],
    )
    .unwrap();
    sql.exec(
        "INSERT INTO items (id, title, stage, size, priority, created_at, updated_at, version, agent) \
         VALUES ('q', 'take the bins out', 'ready', 'quick', 0, 1000, 1000, 1, 0)",
        &[],
    )
    .unwrap();
    sql.exec(
        "INSERT INTO steps (id, item_id, body, done, position, version) \
         VALUES ('s', 'i', 'open the comparison site', 0, 0, 1)",
        &[],
    )
    .unwrap();
    sql.exec(
        "INSERT INTO blocked_by (item_id, blocker_id, version) VALUES ('i', 'q', 1)",
        &[],
    )
    .unwrap();
    sql
}

/// No stash table and no scratch table anywhere — the state a store must be
/// left in whether the rebuild ran, recovered, or declined.
fn assert_no_rebuild_residue(sql: &dyn Sql) {
    for name in ["items_size_rebuild", "steps_fk_stash", "blocked_by_fk_stash", "grills_fk_stash"] {
        assert!(
            sql.exec("SELECT name FROM sqlite_master WHERE name = ?", &[SqlValue::Text(name.to_string())])
                .unwrap()
                .is_empty(),
            "`{name}` was left behind",
        );
    }
}

/// **An attempt that died with the children stood aside and the parent not
/// yet rebuilt.** This is the window the rebuild cannot close with a
/// transaction — the `Sql` seam is one statement at a time and the Durable
/// Object takes no `BEGIN` — so it is closed by being resumable instead, and
/// this test is the proof rather than the claim.
///
/// The cut is placed to catch the failure that actually loses data: the next
/// boot sees `items` still saying `'short'`, runs the rebuild from the top,
/// and would empty the children a second time — over stash tables it had
/// already created. Without recovery that is a `steps_fk_stash already
/// exists` error at best and the permanent loss of every child row at worst.
///
/// `blocked_by` is deliberately left *un*-emptied: an attempt can also die
/// between a child's copy and its `DELETE`, leaving the rows in both places,
/// and the restore has to converge on that rather than fail on its own
/// duplicates. That is what `INSERT OR IGNORE` is for.
#[test]
fn the_size_rebuild_resumes_after_dying_before_the_parent() {
    let migrated = v6_store_with_rows();
    for child in ["steps", "blocked_by", "grills"] {
        migrated
            .exec(&format!("CREATE TABLE {child}_fk_stash AS SELECT * FROM {child}"), &[])
            .unwrap();
    }
    // `steps` got its DELETE; `blocked_by` and `grills` did not.
    migrated.exec("DELETE FROM steps", &[]).unwrap();
    // And a scratch table, half-filled, from the same dead attempt.
    migrated
        .exec("CREATE TABLE items_size_rebuild AS SELECT * FROM items WHERE id = 'q'", &[])
        .unwrap();

    init_schema(&migrated, 0).expect("the next boot resumes rather than failing on its own leftovers");

    let sizes = migrated.exec("SELECT id, size FROM items ORDER BY id", &[]).unwrap();
    assert_eq!(sizes.len(), 2, "the stale scratch copy did not duplicate a row back in");
    assert_eq!(
        sizes[0].get("size"),
        Some(&SqlValue::Text("normal".to_string())),
        "the interrupted rename still completed",
    );
    let steps = migrated.exec("SELECT id FROM steps", &[]).unwrap();
    assert_eq!(steps.len(), 1, "the step the dead attempt had deleted is back");
    let edges = migrated.exec("SELECT item_id FROM blocked_by", &[]).unwrap();
    assert_eq!(edges.len(), 1, "the edge that was in both places did not double");
    assert_no_rebuild_residue(&migrated);
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&RusqliteSql::new()),
        "a resumed store lands on the same byte-identical DDL as a fresh one",
    );
}

/// **An attempt that died after the parent got across.** The dangerous half:
/// `items` now says `'normal'`, so the DDL guard would decline — and a guard
/// that declines while the rows are still in the scratch table and the
/// children still emptied loses all of it *permanently*, on every boot after.
/// Recovery therefore runs before the guard's early return, parent rows
/// first, then the children that reference them.
#[test]
fn the_size_rebuild_resumes_after_dying_after_the_parent() {
    let migrated = v6_store_with_rows();
    init_schema(&migrated, 0).expect("the rebuild succeeds");

    // Wind the store back to the moment between the parent's reinsert and
    // the children's: rows in the scratch table, `items` empty, children
    // stashed and emptied. `items` already carries the new DDL.
    for child in ["steps", "blocked_by", "grills"] {
        migrated
            .exec(&format!("CREATE TABLE {child}_fk_stash AS SELECT * FROM {child}"), &[])
            .unwrap();
        migrated.exec(&format!("DELETE FROM {child}"), &[]).unwrap();
    }
    migrated
        .exec("CREATE TABLE items_size_rebuild AS SELECT * FROM items", &[])
        .unwrap();
    migrated.exec("DELETE FROM items", &[]).unwrap();

    init_schema(&migrated, 0).expect("the boot recovers instead of declining on the new DDL");

    let sizes = migrated.exec("SELECT id, size FROM items ORDER BY id", &[]).unwrap();
    assert_eq!(sizes.len(), 2, "the rows waiting in the scratch table were drained back in");
    assert_eq!(sizes[0].get("size"), Some(&SqlValue::Text("normal".to_string())));
    assert_eq!(
        migrated.exec("SELECT id FROM steps", &[]).unwrap().len(),
        1,
        "the child rows came back after their parents, not instead of them",
    );
    assert_eq!(migrated.exec("SELECT item_id FROM blocked_by", &[]).unwrap().len(), 1);
    assert_no_rebuild_residue(&migrated);
}

/// Running `init_schema` twice over a grown store must not attempt either
/// `ALTER` again — a duplicate column is a hard SQLite error, and
/// `init_schema` runs on every Durable Object construction.
#[test]
fn the_column_migration_is_idempotent() {
    let migrated = v3_store();
    init_schema(&migrated, 0).expect("first growth succeeds");
    init_schema(&migrated, 0).expect("second init is a no-op, not a duplicate-column error");
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

/// `items`, exactly the shape the 6→7 rebuild leaves it in — the finished
/// `'normal'` vocabulary, `agent` inline, byte-identical to the current
/// `CREATE_ITEMS`. Copied rather than derived from [`v6_store`]'s rebuild,
/// because there is no `V6_ADDED_TABLES`-style additive path for a
/// constraint rewrite: this literal *is* what a genuine v7 store's `items`
/// holds.
const V7_ITEMS: &str = "\
CREATE TABLE IF NOT EXISTS items (
  id          TEXT PRIMARY KEY,
  seq         INTEGER UNIQUE,
  title       TEXT NOT NULL CHECK (length(title) > 0),
  description TEXT,
  stage       TEXT NOT NULL CHECK (stage IN
                ('triage','grilling','ready','in_progress','blocked','done')),
  size        TEXT CHECK (size IN ('quick','normal','deep')),
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
, agent INTEGER NOT NULL DEFAULT 0)";

/// A genuine v7 store: [`v6_store`] with `items` swapped for [`V7_ITEMS`] —
/// the same shape the 6→7 rebuild itself produces — and `schema_version`
/// moved to 7. `projects` is still the untouched v2 shape (no
/// `github_repo`/`default_context`), which is exactly what the 7→8 growth
/// below has to find.
///
/// Foreign-key enforcement is toggled off only for the swap: `steps`,
/// `blocked_by` and `grills` all reference `items(id)`, and the fixture
/// carries no rows yet, so there is nothing to stash — unlike the real
/// rebuild in [`rebuild_items_for_size_vocabulary`], which must preserve
/// live data and therefore cannot take this shortcut.
fn v7_store() -> RusqliteSql {
    let sql = v6_store();
    sql.conn
        .pragma_update(None, "foreign_keys", false)
        .expect("FK enforcement toggles off for the fixture-only swap");
    sql.exec("DROP TABLE items", &[]).expect("drop the pre-rebuild items table");
    sql.exec(V7_ITEMS, &[]).expect("v7 items DDL applies");
    for index in [
        "CREATE INDEX IF NOT EXISTS idx_items_version ON items(version)",
        "CREATE INDEX IF NOT EXISTS idx_items_live    ON items(stage) WHERE archived_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_items_project ON items(project_id)",
    ] {
        sql.exec(index, &[]).expect("items index re-applies after the swap");
    }
    sql.conn
        .pragma_update(None, "foreign_keys", true)
        .expect("FK enforcement toggles back on");
    sql.exec("UPDATE meta SET schema_version = 7 WHERE id = 1", &[])
        .expect("v7 meta row seeds");
    sql
}

/// The 7→8 growth path (#625, ADR-0030 decisions 2–3): `projects` gains
/// `github_repo` and `default_context`, the third [`add_missing_columns`]
/// arm — same style as 3→4 and 4→5, not another rebuild, since both new
/// columns are nullable and `projects` carries no `CHECK` to rewrite.
#[test]
fn init_schema_grows_a_schema_7_database_additively() {
    let migrated = v7_store();
    assert_eq!(schema_version(&migrated), 7, "starts genuinely at v7");
    assert!(
        !column_names(&migrated, "projects").contains(&"github_repo".to_string()),
        "the v7 fixture must not already carry github_repo",
    );
    assert!(
        !column_names(&migrated, "projects").contains(&"default_context".to_string()),
        "the v7 fixture must not already carry default_context",
    );
    // A row written before the growth, to prove both ALTERs keep data.
    migrated
        .exec(
            "INSERT INTO projects (id, name, created_at, updated_at, version) \
             VALUES ('p', 'Rebuild the deck', 1000, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(column_names(&migrated, "projects").contains(&"github_repo".to_string()));
    assert!(column_names(&migrated, "projects").contains(&"default_context".to_string()));
    let rows = migrated
        .exec("SELECT id, github_repo, default_context FROM projects", &[])
        .unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");
    assert_eq!(
        rows[0].get("github_repo"),
        Some(&SqlValue::Null),
        "a project minted before #625 names no repo",
    );
    assert_eq!(
        rows[0].get("default_context"),
        Some(&SqlValue::Null),
        "a project minted before #625 names no default context",
    );

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v7 store and a fresh store end up with byte-identical DDL — which is \
         why CREATE_PROJECTS declares both new columns after the newline, before the \
         closing paren, verified against a real migrated store's sqlite_master rather than \
         reasoned out",
    );
}

/// Running `init_schema` a second time over a v7-grown store must not
/// attempt either `ALTER` again, same discipline as
/// [`the_column_migration_is_idempotent`].
#[test]
fn the_project_column_migration_is_idempotent() {
    let migrated = v7_store();
    init_schema(&migrated, 0).expect("first growth succeeds");
    init_schema(&migrated, 0).expect("second init is a no-op, not a duplicate-column error");
    for column in ["github_repo", "default_context"] {
        assert_eq!(
            column_names(&migrated, "projects")
                .iter()
                .filter(|name| *name == column)
                .count(),
            1,
            "exactly one {column} column",
        );
    }
}

/// A genuine v8 store: [`v7_store`] with `projects` carrying 7→8's own
/// `ALTER`s, exactly how a real v8 store came to be. `project_links` is not
/// yet created, which is what the 8→9 growth below has to find.
fn v8_store() -> RusqliteSql {
    let sql = v7_store();
    for ddl in [
        "ALTER TABLE projects ADD COLUMN github_repo TEXT",
        "ALTER TABLE projects ADD COLUMN default_context TEXT",
    ] {
        sql.exec(ddl, &[]).expect("7→8's own ALTER applies");
    }
    sql.exec("UPDATE meta SET schema_version = 8 WHERE id = 1", &[])
        .expect("v8 meta row seeds");
    sql
}

/// The 8→9 growth path (#626, ADR-0030 decision 4): `project_links`, back
/// to the 1→2 / 2→3 / 5→6 shape — a purely additive new table, not another
/// [`add_missing_columns`] arm. `CREATE TABLE IF NOT EXISTS` grows a v8
/// store for free.
#[test]
fn init_schema_grows_a_schema_8_database_additively() {
    let migrated = v8_store();
    assert_eq!(schema_version(&migrated), 8, "starts genuinely at v8");
    assert!(
        !table_names(&migrated).contains(&"project_links".to_string()),
        "the v8 fixture must not already carry project_links"
    );
    // A row written before the growth, to prove the table addition never
    // touches anything pre-existing.
    migrated
        .exec(
            "INSERT INTO projects (id, name, created_at, updated_at, version) \
             VALUES ('p', 'Rebuild the deck', 1000, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(
        table_names(&migrated).contains(&"project_links".to_string()),
        "migrated store missing `project_links`",
    );
    let rows = migrated.exec("SELECT id FROM projects", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");

    let fresh = RusqliteSql::new();
    assert_eq!(
        table_names(&migrated),
        table_names(&fresh),
        "a migrated v8 store and a fresh store end up with identical table sets",
    );
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v8 store and a fresh store end up with byte-identical DDL, \
         including idx_project_links_project (idx_project_links_version was also new here, \
         but #757 drops it from both sides alike)",
    );
}

/// A genuine v9 store: [`v8_store`] with `project_links` created by 8→9's
/// own DDL and `rules` still lacking `deleted_at`, exactly how a real v9
/// store came to be. The DDL here is frozen text, never `CREATE_PROJECT_LINKS`
/// — a fixture that reads today's constant cannot fail when today's
/// constant moves.
fn v9_store() -> RusqliteSql {
    let sql = v8_store();
    for ddl in [
        "\
CREATE TABLE IF NOT EXISTS project_links (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  url         TEXT NOT NULL,
  label       TEXT,
  position    INTEGER NOT NULL,
  removed_at  INTEGER,
  version     INTEGER NOT NULL
)",
        "CREATE INDEX IF NOT EXISTS idx_project_links_version ON project_links(version)",
        "CREATE INDEX IF NOT EXISTS idx_project_links_project ON project_links(project_id)",
    ] {
        sql.exec(ddl, &[]).expect("8→9's own DDL applies");
    }
    sql.exec("UPDATE meta SET schema_version = 9 WHERE id = 1", &[])
        .expect("v9 meta row seeds");
    sql
}

/// The 9→10 growth path: `rules.deleted_at`, the fifth
/// [`add_missing_columns`] arm — back to the 3→4 / 4→5 / 8→9 shape, one
/// nullable column on an existing table, not another rebuild.
///
/// The byte-identity assertion is the load-bearing one, and it is what
/// decides how [`CREATE_RULES`](hummingbird_authority) had to be *written*:
/// `rules` carries no table-level constraint, so `ALTER TABLE … ADD COLUMN`
/// splices immediately before the closing paren, and the fresh store's DDL
/// has to be spelled that same way or the two diverge. Verified against a
/// real migrated store here, not reasoned out.
#[test]
fn init_schema_grows_a_schema_9_database_additively() {
    let migrated = v9_store();
    assert_eq!(schema_version(&migrated), 9, "starts genuinely at v9");
    assert!(
        !column_names(&migrated, "rules").contains(&"deleted_at".to_string()),
        "the v9 fixture must not already carry rules.deleted_at"
    );
    // A row written before the growth, to prove the ALTER never touches
    // anything pre-existing.
    migrated
        .exec(
            "INSERT INTO rules (id, name, conditions, severity, tier, enabled, updated_at, version) \
             VALUES ('r', 'trash slide', '[]', 'high', 'urgent', 1, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    assert!(
        column_names(&migrated, "rules").contains(&"deleted_at".to_string()),
        "migrated store missing `rules.deleted_at`",
    );
    let rows = migrated.exec("SELECT deleted_at FROM rules", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");
    assert!(
        matches!(rows[0].get("deleted_at"), Some(SqlValue::Null) | None),
        "and comes back live, not deleted",
    );

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v9 store and a fresh store end up with byte-identical DDL — which is \
         why CREATE_RULES is written with `, deleted_at INTEGER)` spliced before the closing \
         paren, verified against a real migrated store's sqlite_master rather than reasoned out",
    );
}

#[test]
fn the_rules_deleted_at_migration_is_idempotent() {
    let migrated = v9_store();
    init_schema(&migrated, 0).expect("first growth succeeds");
    init_schema(&migrated, 0).expect("second init is a no-op, not a duplicate-column error");
    assert_eq!(
        column_names(&migrated, "rules")
            .iter()
            .filter(|name| *name == "deleted_at")
            .count(),
        1,
        "exactly one deleted_at column",
    );
}

/// A genuine v10 store: [`v9_store`] with `rules` carrying 9→10's own
/// `ALTER`, exactly how a real v10 store came to be. None of the twelve
/// #289 indexes exist yet, which is what the 10→11 growth below has to add.
fn v10_store() -> RusqliteSql {
    let sql = v9_store();
    sql.exec("ALTER TABLE rules ADD COLUMN deleted_at INTEGER", &[])
        .expect("9→10's own ALTER applies");
    sql.exec("UPDATE meta SET schema_version = 10 WHERE id = 1", &[])
        .expect("v10 meta row seeds");
    sql
}

/// The 10→11 growth path (#289): twelve version-leading indexes, one per
/// synced table, purely additive — `CREATE INDEX IF NOT EXISTS` grows a v10
/// store for free, same style as [`init_schema_grows_a_schema_5_database_additively`]
/// and every other pure-index/table growth. `projects` had no version index
/// at all before this; `items` already had a bare one and gains a second,
/// differently-named compound one alongside it.
#[test]
fn init_schema_grows_a_schema_10_database_additively() {
    let migrated = v10_store();
    assert_eq!(schema_version(&migrated), 10, "starts genuinely at v10");
    assert!(
        !has_version_leading_index(&migrated, "projects"),
        "the v10 fixture must not already carry a projects version index",
    );
    // A row written before the growth, to prove the new indexes never touch
    // anything pre-existing.
    migrated
        .exec(
            "INSERT INTO projects (id, name, created_at, updated_at, version) \
             VALUES ('p', 'Rebuild the deck', 1000, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    for table in [
        "projects",
        "routes",
        "fog",
        "blocked_by",
        "alerts",
        "context_snapshots",
        "settings",
        "items",
        "steps",
        "rules",
        "grills",
        "project_links",
    ] {
        assert!(
            has_version_leading_index(&migrated, table),
            "migrated store missing a version-leading index on `{table}`",
        );
    }
    let rows = migrated.exec("SELECT id FROM projects", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v10 store and a fresh store end up with byte-identical DDL, \
         including all twelve new indexes",
    );
}

#[test]
fn the_version_leading_index_growth_is_idempotent() {
    let migrated = v10_store();
    init_schema(&migrated, 0).expect("first growth succeeds");
    init_schema(&migrated, 0).expect("second init is a no-op, not a duplicate-index error");
    assert_eq!(schema_version(&migrated), SCHEMA_VERSION);
}

/// A genuine v11 store: [`v10_store`] with #289's twelve version-leading
/// indexes applied by their own frozen DDL, exactly as the 10→11 growth
/// produced them — including the five bare `idx_<table>_version` indexes
/// standing alongside their `_version_id` compound, which is what the 11→12
/// growth below has to drop.
fn v11_store() -> RusqliteSql {
    let sql = v10_store();
    for index in [
        "CREATE INDEX IF NOT EXISTS idx_projects_version ON projects(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_routes_version ON routes(version, project_id)",
        "CREATE INDEX IF NOT EXISTS idx_fog_version ON fog(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_blocked_by_version ON blocked_by(version, item_id, blocker_id)",
        "CREATE INDEX IF NOT EXISTS idx_alerts_version ON alerts(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_context_snapshots_version ON context_snapshots(version, source, key)",
        "CREATE INDEX IF NOT EXISTS idx_settings_version ON settings(version, key)",
        "CREATE INDEX IF NOT EXISTS idx_items_version_id ON items(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_steps_version_id ON steps(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_rules_version_id ON rules(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_grills_version_id ON grills(version, id)",
        "CREATE INDEX IF NOT EXISTS idx_project_links_version_id ON project_links(version, id)",
    ] {
        sql.exec(index, &[]).expect("10→11's own DDL applies");
    }
    sql.exec("UPDATE meta SET schema_version = 11 WHERE id = 1", &[])
        .expect("v11 meta row seeds");
    sql
}

/// The names 11→12 (#757) removes — a same-table `_version_id` compound
/// prefix-subsumes each.
const DROPPED_BARE_VERSION_INDEXES: [&str; 5] = [
    "idx_items_version",
    "idx_steps_version",
    "idx_rules_version",
    "idx_grills_version",
    "idx_project_links_version",
];

/// The seven plain-named indexes #289 introduced that are themselves the
/// compound — #757 must leave every one of these standing.
const UNTOUCHED_PLAIN_COMPOUND_INDEXES: [&str; 7] = [
    "idx_projects_version",
    "idx_routes_version",
    "idx_fog_version",
    "idx_blocked_by_version",
    "idx_alerts_version",
    "idx_context_snapshots_version",
    "idx_settings_version",
];

/// The 11→12 growth path (#757): drop the five bare `idx_<table>_version`
/// indexes a same-table `_version_id` compound has prefix-subsumed since
/// #289. Unlike every other growth step in this file, there is no
/// `CREATE … IF NOT EXISTS` that reaches this state for free — it is a
/// `DROP`, so it is asserted here by name, not by the byte-identity check
/// alone: removing an entry from `CREATE_INDEXES` would silently leave a
/// migrated store's copy standing forever, and the byte-identity assertion
/// below only proves the migrated store matches a fresh one, not which
/// index names got there.
#[test]
fn init_schema_grows_a_schema_11_database_additively() {
    let migrated = v11_store();
    assert_eq!(schema_version(&migrated), 11, "starts genuinely at v11");
    for index in DROPPED_BARE_VERSION_INDEXES {
        assert!(
            index_names(&migrated).contains(&index.to_string()),
            "the v11 fixture must still carry `{index}`",
        );
    }
    // A row written before the growth, to prove the drop never touches data.
    migrated
        .exec(
            "INSERT INTO items (id, title, stage, created_at, updated_at, version) \
             VALUES ('i', 'Empty the compost', 'ready', 1000, 1000, 1)",
            &[],
        )
        .unwrap();

    init_schema(&migrated, 0).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");
    let names = index_names(&migrated);
    for index in DROPPED_BARE_VERSION_INDEXES {
        assert!(!names.contains(&index.to_string()), "`{index}` should have been dropped");
    }
    for index in UNTOUCHED_PLAIN_COMPOUND_INDEXES {
        assert!(names.contains(&index.to_string()), "`{index}` must survive untouched");
    }
    for index in [
        "idx_items_version_id",
        "idx_steps_version_id",
        "idx_rules_version_id",
        "idx_grills_version_id",
        "idx_project_links_version_id",
    ] {
        assert!(names.contains(&index.to_string()), "`{index}` compound must survive");
    }
    let rows = migrated.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the pre-growth row survives");

    let fresh = RusqliteSql::new();
    assert_eq!(
        schema_ddl(&migrated),
        schema_ddl(&fresh),
        "a migrated v11 store and a fresh store end up with byte-identical DDL, \
         with none of the five dropped bare indexes on either side",
    );
}

#[test]
fn the_bare_version_index_drop_is_idempotent() {
    let migrated = v11_store();
    init_schema(&migrated, 0).expect("first growth succeeds");
    init_schema(&migrated, 0).expect("second init is a no-op, not a missing-index error");
    assert_eq!(schema_version(&migrated), SCHEMA_VERSION);
}

fn index_names(sql: &dyn Sql) -> Vec<String> {
    sql.exec(
        "SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| r.get("name").unwrap().as_text().unwrap().to_string())
    .collect()
}

// ------------------------------- #289: version-leading indexes

/// Tables outside the delta-pull contract (`handlers/changes.rs`'s own
/// header): `meta` is the counter itself, `tokens`/`push_targets`/
/// `deliveries` carry no `version` column at all. Everything else in
/// `sqlite_master` is a synced table and must be covered — read off the
/// schema itself rather than hand-kept, so a future synced table is caught
/// by this test without anyone remembering to add it here.
const NOT_SYNCED: &[&str] = &["meta", "tokens", "push_targets", "deliveries"];

/// Whether `table` carries an index whose leading column is `version` —
/// `PRAGMA index_info`'s first row, not a text search over the DDL, so a
/// column named e.g. `version_note` cannot false-positive it.
fn has_version_leading_index(sql: &dyn Sql, table: &str) -> bool {
    sql.exec(&format!("PRAGMA index_list({table})"), &[])
        .unwrap()
        .iter()
        .any(|idx| {
            let name = idx.get("name").unwrap().as_text().unwrap();
            sql.exec(&format!("PRAGMA index_info({name})"), &[])
                .unwrap()
                .first()
                .and_then(|col| col.get("name"))
                .and_then(SqlValue::as_text)
                == Some("version")
        })
}

/// Acceptance criterion 1: every synced table's delta pull is served by a
/// version-leading index — all ten-plus synced tables, not only the seven
/// the issue named, asserted from the schema rather than a hand-kept list.
#[test]
fn every_synced_table_has_a_version_leading_index() {
    let sql = RusqliteSql::new();
    for table in table_names(&sql) {
        if NOT_SYNCED.contains(&table.as_str()) {
            continue;
        }
        assert!(
            has_version_leading_index(&sql, &table),
            "`{table}` is synced (delta-pulled) but has no version-leading index",
        );
    }
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

/// `items`' own stored `CREATE` statement — the size vocabulary lives in
/// its `CHECK`, which no column-level helper can see.
fn items_ddl(sql: &dyn Sql) -> String {
    schema_ddl(sql)
        .into_iter()
        .find(|(name, _)| name == "items")
        .expect("`items` is in sqlite_master")
        .1
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

// ------------------------------- #549: the unaddressable-row catch-up

/// The row #549 is about: minted through the old `POST /api/items`, which
/// checked only that the id was non-empty. Seeded directly because the
/// route it came through no longer exists in that shape (#548).
fn seed_unaddressable(sql: &RusqliteSql, id: &str) {
    sql.exec(
        "INSERT INTO items (id, title, stage, priority, agent, version, created_at, updated_at) \
         VALUES (?, 'test mint', 'ready', 0, 0, 1, 1787064577642, 1787064577642)",
        &[SqlValue::Text(id.to_string())],
    )
    .expect("seed");
}

#[test]
fn init_schema_archives_a_row_no_path_could_ever_address() {
    let sql = RusqliteSql::new();
    seed_unaddressable(&sql, "test mint 526 repro B");
    let before = meta_version(&sql);

    init_schema(&sql, 9_000).expect("growth init succeeds");

    let rows = sql
        .exec(
            "SELECT archived_at, updated_at, version FROM items WHERE id = 'test mint 526 repro B'",
            &[],
        )
        .unwrap();
    let row = rows.first().expect("the row is still there — archived, not deleted (ADR-0020)");
    assert_eq!(
        row.get("archived_at").unwrap().as_i64(),
        Some(9_000),
        "stamped with the boot's clock"
    );
    assert_eq!(row.get("updated_at").unwrap().as_i64(), Some(9_000));
    assert_eq!(
        row.get("version").unwrap().as_i64(),
        Some(before + 1),
        "a real version off the workspace counter, so the delta pull carries it"
    );
    assert_eq!(meta_version(&sql), before + 1, "and the counter moved with it");
}

#[test]
fn the_catch_up_leaves_every_addressable_row_alone() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 1000);
    let before = meta_version(&sql);

    init_schema(&sql, 9_000).expect("growth init succeeds");

    let rows = sql
        .exec("SELECT archived_at, version FROM items WHERE id = 'a-1'", &[])
        .unwrap();
    let row = rows.first().expect("row survives");
    assert!(row.get("archived_at").unwrap().as_i64().is_none(), "still live");
    assert_eq!(meta_version(&sql), before, "a clean store writes nothing at all");
}

/// Every boot re-runs `init_schema`, so the catch-up must settle rather
/// than keep re-archiving — a second stamp would bump the counter forever
/// and re-push the row to every device on each boot.
#[test]
fn the_catch_up_is_a_no_op_on_the_second_boot() {
    let sql = RusqliteSql::new();
    seed_unaddressable(&sql, "test mint 526 repro B");
    init_schema(&sql, 9_000).expect("first boot archives");
    let after_first = meta_version(&sql);

    init_schema(&sql, 20_000).expect("second boot");

    let rows = sql
        .exec(
            "SELECT archived_at FROM items WHERE id = 'test mint 526 repro B'",
            &[],
        )
        .unwrap();
    assert_eq!(
        rows.first().unwrap().get("archived_at").unwrap().as_i64(),
        Some(9_000),
        "the original stamp stands"
    );
    assert_eq!(meta_version(&sql), after_first, "and nothing was written twice");
}
