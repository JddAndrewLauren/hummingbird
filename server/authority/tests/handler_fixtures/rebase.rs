//! The rebase-on-409 contract, end-to-end (#114 acceptance criterion 2):
//! the server adjudicates by version; the *client's* half — comparing
//! touched fields against the carried entity — is performed inline here,
//! exactly as S3 (#101) will implement it.

use hummingbird_domain::{ConflictResponse, Item};

use crate::rig::*;

#[test]
fn disjoint_field_rebase_end_to_end() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello", "description": "original"}"#, 0); // v1

    // Writer A renames at v1 and wins.
    let resp = patch(&sql, "a-1", r#"{"expected_version": 1, "title": "renamed"}"#, 0);
    assert_eq!(resp.status, 200);

    // Writer B edits the description, also at v1 — stale.
    let stale = patch(
        &sql,
        "a-1",
        r#"{"expected_version": 1, "description": "B's edit"}"#,
        0,
    );
    assert_eq!(stale.status, 409);
    let conflict: ConflictResponse = body_as(&stale);
    let current = conflict.current;
    assert_eq!(current.version, 2, "the 409 carries the winning state");

    // B's client-side rebase: the field B touched (description) still holds
    // the value B based its edit on — the conflict is disjoint — so B
    // resends against the carried version.
    assert_eq!(
        current.description.as_deref(),
        Some("original"),
        "B's touched field is unchanged on the server: disjoint conflict"
    );
    let resend = patch(
        &sql,
        "a-1",
        &format!(
            r#"{{"expected_version": {}, "description": "B's edit"}}"#,
            current.version
        ),
        0,
    );
    assert_eq!(resend.status, 200, "{}", resend.body);
    let merged: Item = body_as(&resend);
    assert_eq!(merged.title, "renamed", "A's edit survives");
    assert_eq!(merged.description.as_deref(), Some("B's edit"), "B's edit lands");
    assert_eq!(merged.version, 3);
}

#[test]
fn same_field_conflict_carries_the_winner_for_the_dead_letter_journal() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 0); // v1

    patch(&sql, "a-1", r#"{"expected_version": 1, "title": "A's title"}"#, 0);
    let stale = patch(&sql, "a-1", r#"{"expected_version": 1, "title": "B's title"}"#, 0);
    assert_eq!(stale.status, 409);
    let conflict: ConflictResponse = body_as(&stale);

    // B touched `title` and the carried entity shows it already moved:
    // same-field conflict — B's edit loses into the client-side dead-letter
    // journal (ADR-0007/0008). The server's whole contribution is this 409
    // and the current entity; nothing was written.
    assert_eq!(conflict.current.title, "A's title");
    assert_eq!(conflict.current.version, 2);
    assert_eq!(meta_version(&sql), 2, "the losing write left no trace");
}

/// Crash-replay of an *applied* CAS write: the retry conflicts (409), and
/// the carried entity already holds the values the retry wanted — the
/// client recognizes its own write and drops the retry. Absolute-value
/// sets are what make that recognition possible.
#[test]
fn crash_replay_of_an_applied_patch_conflicts_with_its_own_result() {
    let sql = RusqliteSql::new();
    post(&sql, r#"{"id": "a-1", "title": "hello"}"#, 0); // v1
    let body = r#"{"expected_version": 1, "title": "renamed"}"#;
    assert_eq!(patch(&sql, "a-1", body, 0).status, 200);

    let replay = patch(&sql, "a-1", body, 0);
    assert_eq!(replay.status, 409, "the replayed CAS write is stale by its own success");
    let conflict: ConflictResponse = body_as(&replay);
    assert_eq!(
        conflict.current.title, "renamed",
        "the carried entity already holds the replayed value — a recognizable no-op"
    );
    assert_eq!(meta_version(&sql), 2, "no double-apply");
}
