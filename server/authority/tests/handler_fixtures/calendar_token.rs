//! `POST /api/google/calendar_token` (#577/#582): the DO's authorization
//! verdict, mirroring `skills.rs`'s own fixture suite for the same shape.
//!
//! The exchange with `oauth2.googleapis.com` and the DO-instance cache live
//! in the `wasm32` shim and cannot be tested natively; what is proven here
//! is everything the shim is forbidden to decide for itself: who reaches
//! the verdict, that reaching it writes nothing, and which method/path
//! combinations are 404 vs. 405.

use crate::rig::*;

const CALENDAR_TOKEN: &str = "/api/google/calendar_token";

#[test]
fn a_device_token_reaches_the_verdict() {
    let sql = RusqliteSql::new();
    let resp = req_as(&sql, DEVICE_TOKEN, "POST", CALENDAR_TOKEN, None, None, 1_000);
    assert_eq!(resp.status, 204);
    assert_eq!(resp.body, "");
    assert!(resp.deliveries.is_empty());
}

/// The route is device-only through `permitted`'s default arm — no arm of
/// its own, which would be dead code duplicating the default.
#[test]
fn every_other_scope_is_out_of_scope() {
    let sql = RusqliteSql::new();
    for token in [SWEEPER_TOKEN, INGEST_TOKEN] {
        let resp = req_as(&sql, token, "POST", CALENDAR_TOKEN, None, None, 1_000);
        assert_eq!(resp.status, 403, "{token} should be out of scope");
        assert_eq!(resp.body, "", "403 must leak no body");
    }
}

#[test]
fn an_unauthenticated_or_garbage_caller_is_a_clean_401() {
    let sql = RusqliteSql::new();
    let anon = req_anon(&sql, "POST", CALENDAR_TOKEN, None, None);
    assert_eq!(anon.status, 401);
    assert_eq!(anon.body, "");

    let garbage = req_as(&sql, "hb_not_a_real_token", "POST", CALENDAR_TOKEN, None, None, 1_000);
    assert_eq!(garbage.status, 401);
    assert_eq!(garbage.body, "");
}

#[test]
fn the_wrong_method_is_a_405_not_a_404() {
    let sql = RusqliteSql::new();
    for method in ["GET", "DELETE", "PATCH", "PUT"] {
        let resp = req_as(&sql, DEVICE_TOKEN, method, CALENDAR_TOKEN, None, None, 1_000);
        assert_eq!(resp.status, 405, "{method} {CALENDAR_TOKEN}");
    }
}

/// A tap must not dirty the sync cursor. `authenticate` stamps `last_seen`
/// without bumping `meta` on purpose; this pins that the verdict adds
/// nothing on top of it.
#[test]
fn the_verdict_does_not_bump_meta_version() {
    let sql = RusqliteSql::new();
    seed_item(&sql, "item-1");
    let before = meta_version(&sql);
    req_as(&sql, DEVICE_TOKEN, "POST", CALENDAR_TOKEN, None, None, 1_000);
    req_as(&sql, DEVICE_TOKEN, "POST", CALENDAR_TOKEN, None, None, 2_000);
    assert_eq!(meta_version(&sql), before);
}
