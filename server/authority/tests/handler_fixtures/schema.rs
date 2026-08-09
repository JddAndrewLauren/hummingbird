//! Schema lifecycle: idempotent init, the full table set, and the additive
//! 1→2 growth path.

use hummingbird_authority::init_schema;

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

    let schema_version = sql
        .exec("SELECT schema_version FROM meta WHERE id = 1", &[])
        .unwrap()[0]
        .get("schema_version")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(schema_version, 2, "schema_version moved forward");
    assert_eq!(meta_version(&sql), 1, "the workspace counter is untouched");
    let rows = sql.exec("SELECT id FROM items", &[]).unwrap();
    assert_eq!(rows.len(), 1, "existing rows survive the growth");
}
