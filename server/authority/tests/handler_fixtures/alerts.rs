//! The alerts lane: version-blind ingest upsert on `(source, source_key)`
//! with a deterministic id, and the device-scoped dismiss patch.

use hummingbird_domain::{Alert, ConflictResponse};

use crate::rig::*;

#[test]
fn first_raise_201_with_deterministic_id_and_bump() {
    let sql = RusqliteSql::new();
    let resp = ingest_alert(
        &sql,
        r#"{"source": "healthchecks", "source_key": "sweeper", "title": "sweeper is down",
            "severity": "high", "url": "https://hc.example/check/1"}"#,
        1000,
    );
    assert_eq!(resp.status, 201, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    // The preimage length-prefixes the source, so shifted-colon identities
    // cannot collide (see `identity_shifted_colon_mints_distinct_ids`).
    let expected_id = &sha256_hex("alert:12:healthchecks:sweeper")[..32];
    assert_eq!(alert.id, expected_id, "id is a pure function of the identity");
    assert_eq!(alert.raised_at, 1000, "server clock fills an absent raised_at");
    assert_eq!(alert.dismissed_at, None);
    assert_eq!(alert.version, 1);
    assert_eq!(meta_version(&sql), 1);
}

#[test]
fn identity_shifted_colon_mints_distinct_ids() {
    let sql = RusqliteSql::new();
    // Without the length prefix in the id preimage, these two identities
    // would hash to the same id — and the second first-raise would 500 on
    // the alerts primary key, forever.
    let first = ingest_alert(
        &sql,
        r#"{"source": "app", "source_key": "db:prod", "title": "down"}"#,
        0,
    );
    assert_eq!(first.status, 201, "{}", first.body);
    let second = ingest_alert(
        &sql,
        r#"{"source": "app:db", "source_key": "prod", "title": "down"}"#,
        0,
    );
    assert_eq!(second.status, 201, "{}", second.body);
    let a: Alert = body_as(&first);
    let b: Alert = body_as(&second);
    assert_ne!(a.id, b.id, "distinct identities mint distinct ids");
    let rows = sql.exec("SELECT id FROM alerts", &[]).unwrap();
    assert_eq!(rows.len(), 2, "two rows, one per identity");
}

#[test]
fn identical_re_raise_is_a_noop() {
    let sql = RusqliteSql::new();
    let body = r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "high"}"#;
    ingest_alert(&sql, body, 1000);
    let resp = ingest_alert(&sql, body, 2000);
    assert_eq!(resp.status, 200, "re-raise is success, not conflict");
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.raised_at, 1000, "absent raised_at keeps the stored stamp");
    assert_eq!(alert.version, 1, "identical payload = no write");
    assert_eq!(meta_version(&sql), 1, "no bump — a crash-replay is a no-op");
    let rows = sql.exec("SELECT id FROM alerts", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the upsert landed on one row");
}

#[test]
fn changed_re_raise_updates_source_fields_absolutely() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "body": "details", "severity": "high"}"#,
        1000,
    );
    // The source resolves the alert and stops sending body/severity. `body`
    // clears — the source is authoritative for its own fields. `severity`
    // is the ADR-0014 exception: the row was still live going into this
    // call, so an absent severity ratchets (stays at its rank, never
    // clears) rather than blind-overwriting to `None` like every other
    // optional (see `severity_clears_once_the_alert_has_left_live` for the
    // contrasting case).
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "resolved_at": 5000}"#,
        6000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.resolved_at, Some(5000));
    assert_eq!(alert.body, None, "absent optional clears");
    assert_eq!(alert.severity, Some("high".into()), "severity ratchets, it does not clear, while live");
    assert_eq!(alert.version, 2, "a real change bumps");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn severity_clears_once_the_alert_has_left_live() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "high"}"#,
        1000,
    );
    // Resolve it — the row leaves live.
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "high", "resolved_at": 2000}"#,
        2000,
    );
    // A later raise (a fresh occurrence) that omits severity clears it,
    // exactly like every other source-owned optional — the ratchet
    // exception applies only while the row it is guarding is still live.
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down again"}"#,
        3000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.severity, None, "a fresh occurrence's payload is authoritative as-is");
}

/// ADR-0014's severity ratchet: a `normal` mint against a live `urgent`
/// alert must not downgrade it.
#[test]
fn normal_mint_does_not_downgrade_a_live_urgent_alert() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "urgent"}"#,
        1000,
    );
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "still down", "severity": "normal"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.severity, Some("urgent".into()), "a lower severity must never win the ratchet");
}

/// The mirror case: an `urgent` mint against a live `normal` alert raises
/// it.
#[test]
fn urgent_mint_escalates_a_live_normal_alert() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "normal"}"#,
        1000,
    );
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "still down", "severity": "urgent"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.severity, Some("urgent".into()), "a higher severity escalates");
}

/// An unranked severity string (outside `hummingbird_domain::SEVERITIES`)
/// has defined behaviour: it neither panics nor wins a ratchet it did not
/// earn.
#[test]
fn an_unranked_severity_does_not_win_the_ratchet_or_panic() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "severity": "normal"}"#,
        1000,
    );
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "still down", "severity": "something-weird"}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(
        alert.severity,
        Some("normal".into()),
        "an unranked incoming severity must not displace a known one"
    );
}

/// ADR-0014's live predicate, exercised end to end: a dismissal at T, then
/// a replay of the *original* raise (stamped at or before T), stays quiet.
#[test]
fn dismissed_alert_replaying_the_original_raise_stays_dismissed() {
    let sql = RusqliteSql::new();
    let alert: Alert = body_as(&ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down"}"#,
        1000,
    ));
    patch_at(
        &sql,
        &format!("/api/alerts/{}", alert.id),
        r#"{"expected_version": 1, "dismissed_at": 2000}"#,
        2000,
    );

    // A crash-and-replay of the exact original payload: raised_at is
    // omitted, so the stored stamp (1000) is kept — before the dismissal.
    let resp = ingest_alert(&sql, r#"{"source": "hc", "source_key": "k", "title": "down"}"#, 3000);
    let replayed: Alert = body_as(&resp);
    assert!(
        !replayed.is_live(3000),
        "a replay of the dismissed raise must stay quiet"
    );
    assert_eq!(replayed.dismissed_at, Some(2000));
}

/// The mirror case: a dismissal at T, then a re-raise stamped *after* T,
/// is a later occurrence and rings again.
#[test]
fn dismissed_alert_re_raised_after_the_dismissal_is_live_again() {
    let sql = RusqliteSql::new();
    let alert: Alert = body_as(&ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down"}"#,
        1000,
    ));
    patch_at(
        &sql,
        &format!("/api/alerts/{}", alert.id),
        r#"{"expected_version": 1, "dismissed_at": 2000}"#,
        2000,
    );

    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down again", "raised_at": 3000}"#,
        3000,
    );
    let re_raised: Alert = body_as(&resp);
    assert!(re_raised.is_live(3000), "a raise stamped after the dismissal must ring again");
    assert_eq!(re_raised.dismissed_at, Some(2000), "the human-owned stamp is still never touched");
}

#[test]
fn re_raise_omitting_expires_at_clears_it() {
    let sql = RusqliteSql::new();
    ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down", "expires_at": 9000}"#,
        1000,
    );
    // The source stops sending its TTL: absent clears, like every other
    // source-owned optional — and a cleared field is a real change.
    let resp = ingest_alert(&sql, r#"{"source": "hc", "source_key": "k", "title": "down"}"#, 2000);
    assert_eq!(resp.status, 200, "{}", resp.body);
    let alert: Alert = body_as(&resp);
    assert_eq!(alert.expires_at, None, "absent expires_at clears");
    assert_eq!(alert.version, 2, "clearing is a real change");
    assert_eq!(meta_version(&sql), 2);
}

#[test]
fn ingest_never_touches_the_human_owned_dismissal() {
    let sql = RusqliteSql::new();
    ingest_alert(&sql, r#"{"source": "hc", "source_key": "k", "title": "down"}"#, 1000);
    let alert: Alert = body_as(&ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down"}"#,
        1000,
    ));

    // The human dismisses it (device scope).
    let resp = patch_at(
        &sql,
        &format!("/api/alerts/{}", alert.id),
        r#"{"expected_version": 1, "dismissed_at": 2000}"#,
        2000,
    );
    assert_eq!(resp.status, 200, "{}", resp.body);

    // A re-raise with new content updates the source fields but the
    // dismissal stands — re-raise-clears-dismissal is per-source policy,
    // deferred to wiring time; the server stays dumb.
    let resp = ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "still down"}"#,
        3000,
    );
    let re_raised: Alert = body_as(&resp);
    assert_eq!(re_raised.title, "still down");
    assert_eq!(re_raised.dismissed_at, Some(2000), "dismissal survives ingest");
}

#[test]
fn dismiss_is_cas_set_and_clear() {
    let sql = RusqliteSql::new();
    let alert: Alert = body_as(&ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down"}"#,
        0,
    ));
    let path = format!("/api/alerts/{}", alert.id);

    let resp = patch_at(&sql, &path, r#"{"expected_version": 1, "dismissed_at": 500}"#, 0);
    let dismissed: Alert = body_as(&resp);
    assert_eq!(dismissed.dismissed_at, Some(500));
    assert_eq!(dismissed.version, 2);

    let resp = patch_at(&sql, &path, r#"{"expected_version": 2, "dismissed_at": null}"#, 0);
    let undismissed: Alert = body_as(&resp);
    assert_eq!(undismissed.dismissed_at, None, "explicit null un-dismisses");

    let resp = patch_at(&sql, &path, r#"{"expected_version": 1, "dismissed_at": 9}"#, 0);
    assert_eq!(resp.status, 409);
    let conflict: ConflictResponse<Alert> = body_as(&resp);
    assert_eq!(conflict.current.version, 3);

    let resp = patch_at(&sql, "/api/alerts/ghost", r#"{"expected_version": 1}"#, 0);
    assert_eq!(resp.status, 404);
}

#[test]
fn dismiss_cannot_write_source_owned_fields() {
    let sql = RusqliteSql::new();
    let alert: Alert = body_as(&ingest_alert(
        &sql,
        r#"{"source": "hc", "source_key": "k", "title": "down"}"#,
        0,
    ));
    let resp = patch_at(
        &sql,
        &format!("/api/alerts/{}", alert.id),
        r#"{"expected_version": 1, "title": "rewritten"}"#,
        0,
    );
    assert_eq!(resp.status, 400, "the patch DTO carries dismissed_at only: {}", resp.body);
}

#[test]
fn ingest_validation_400() {
    let sql = RusqliteSql::new();
    for (body, why) in [
        (r#"{"source": "", "source_key": "k", "title": "t"}"#, "empty source"),
        (r#"{"source": "s", "source_key": "", "title": "t"}"#, "empty source_key"),
        (r#"{"source": "s", "source_key": "k", "title": ""}"#, "empty title"),
        (
            r#"{"source": "s", "source_key": "k", "title": "t", "dismissed_at": 1}"#,
            "dismissed_at is human-owned",
        ),
    ] {
        let resp = ingest_alert(&sql, body, 0);
        assert_eq!(resp.status, 400, "{why}: {}", resp.body);
    }
    assert_eq!(meta_version(&sql), 0);
}
