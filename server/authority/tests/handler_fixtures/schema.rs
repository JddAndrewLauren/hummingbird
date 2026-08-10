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

/// The 2→3 growth path (#131): a schema-2 database (the full pre-notification
/// ADR-0009 DDL) gains `rules`, `push_targets` and `deliveries` additively,
/// and ends up with a schema identical to a fresh store's — same table set,
/// same `schema_version`.
#[test]
fn init_schema_grows_a_schema_2_database_additively() {
    let migrated = RusqliteSql::new();
    sql_exec_ok(&migrated, "UPDATE meta SET schema_version = 2 WHERE id = 1");

    init_schema(&migrated).expect("growth init succeeds");

    assert_eq!(schema_version(&migrated), SCHEMA_VERSION, "schema_version moved forward");

    let fresh = RusqliteSql::new();
    assert_eq!(
        table_names(&migrated),
        table_names(&fresh),
        "a migrated v2 store and a fresh store end up with identical table sets",
    );
    for table in ["rules", "push_targets", "deliveries"] {
        assert!(
            table_names(&migrated).contains(&table.to_string()),
            "migrated store missing `{table}`",
        );
    }
}

fn sql_exec_ok(sql: &dyn Sql, stmt: &str) {
    sql.exec(stmt, &[]).unwrap();
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
