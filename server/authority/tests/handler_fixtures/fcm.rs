//! `hummingbird_authority::revoke_dead_target` (#219): the one write the
//! FCM send leg is allowed to make, against real SQLite.
//!
//! Everything else about the send — the assertion, the message body, what a
//! response means — is pure and unit-tested in `authority/src/fcm.rs`. This
//! file covers only the part that touches the `push_targets` table, and in
//! particular that a revocation from the send path composes correctly with
//! `POST`/`DELETE /api/push_targets`, which the operator drives
//! independently.

use hummingbird_authority::{revoke_dead_target, Sql, SqlValue};

use crate::rig::*;

fn revoked_at(sql: &dyn Sql, id: &str) -> Option<i64> {
    let rows = sql
        .exec(
            "SELECT revoked_at FROM push_targets WHERE id = ?",
            &[SqlValue::Text(id.into())],
        )
        .unwrap();
    rows[0].get("revoked_at").unwrap().as_i64()
}

#[test]
fn an_unregistered_token_revokes_its_target() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    assert_eq!(revoked_at(&sql, "pt-1"), None);

    revoke_dead_target(&sql, "pt-1", 900).unwrap();

    assert_eq!(revoked_at(&sql, "pt-1"), Some(900));
}

/// The revoked row must stop receiving pushes — that is the whole point,
/// and `deliver` selects on exactly this predicate.
#[test]
fn a_revoked_target_drops_out_of_the_live_target_set() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    seed_push_target_raw(&sql, "pt-2", "pixel-watch");

    revoke_dead_target(&sql, "pt-1", 900).unwrap();

    let live = sql
        .exec("SELECT id FROM push_targets WHERE revoked_at IS NULL", &[])
        .unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].get("id").unwrap().as_text(), Some("pt-2"));
}

/// Two dead-token responses in one tick (two rules ringing the same device)
/// must not fight, and must not move the stamp the first one wrote.
#[test]
fn revoking_an_already_revoked_target_keeps_the_original_stamp() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");

    revoke_dead_target(&sql, "pt-1", 900).unwrap();
    revoke_dead_target(&sql, "pt-1", 1_500).unwrap();

    assert_eq!(
        revoked_at(&sql, "pt-1"),
        Some(900),
        "the send path reports that the device is gone, not when it went",
    );
}

/// A target the operator already revoked by hand keeps *their* timestamp:
/// the send path never overwrites a deliberate act with an incidental one.
#[test]
fn a_hand_revoked_target_keeps_the_operator_s_stamp() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    sql.exec(
        "UPDATE push_targets SET revoked_at = 100 WHERE id = ?",
        &[SqlValue::Text("pt-1".into())],
    )
    .unwrap();

    revoke_dead_target(&sql, "pt-1", 900).unwrap();

    assert_eq!(revoked_at(&sql, "pt-1"), Some(100));
}

/// A target row that vanished between `deliver` reading it and the send
/// finishing is not an error — there is nothing left to revoke.
#[test]
fn revoking_an_unknown_target_is_a_no_op() {
    let sql = RusqliteSql::new();
    revoke_dead_target(&sql, "pt-missing", 900).unwrap();
}

/// The device comes back: `POST /api/push_targets` revives it, exactly as
/// it does after a hand revocation (`push_targets::register`'s stated
/// contract). A dead token must not be a one-way door.
#[test]
fn re_registering_revives_a_target_the_send_path_revoked() {
    let sql = RusqliteSql::new();
    seed_push_target_raw(&sql, "pt-1", "pixel-9");
    revoke_dead_target(&sql, "pt-1", 900).unwrap();

    let response = post_to(
        &sql,
        "/api/push_targets",
        r#"{"id":"pt-1","name":"pixel-9","platform":"android","fcm_token":"fresh-token"}"#,
        1_500,
    );

    assert_eq!(response.status, 200);
    assert_eq!(revoked_at(&sql, "pt-1"), None);
}
