//! **The cross-language contract, and the only guard against it drifting.**
//!
//! The body inside ADR-0015's envelope is deliberately unfrozen and opaque
//! to the server: `SnapshotEnvelope` carries it through as text, `POST
//! /api/snapshots` never looks inside it, and the pane's own parser is what
//! pins its shape. So this file asserts the literal snake_case key names
//! twice — once on the JSON this crate actually produces, and once against
//! the text of the code that consumes it — on `kimi-balance/tests/
//! contract.rs`'s own reasoning: **no type, no schema and no compiler on
//! either side can see the other**, and a rename made on only one side
//! would otherwise still pass every test in both languages while the pane
//! silently read "no answer yet" forever.
//!
//! **Retargeted at #534.** ADR-0025/#534 sank the pane's parser out of
//! `client/web/src/screens/uptime-pane/uptime.ts` and into
//! `client/core/src/decisions/panes/uptime.rs::parse_uptime_body` — the
//! real parse surface now lives there (`uptime.ts` kept its name but is now
//! a thin rendering wrapper over the seam, and no longer spells `body.…`
//! anywhere).

use hummingbird_uptime_probe::body::{ProbeBody, POLLED_EVERY_MS};
use hummingbird_uptime_probe::manifest::{parse_manifest, Service, SERVICES_JSON};
use hummingbird_uptime_probe::verdict::Outcome;

/// Every key the pane reads out of the body, spelled exactly as it appears
/// on the wire.
const KEYS_THE_PANE_READS: &[&str] = &["expected", "expect_status", "observed_status", "error"];

const UPTIME_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../client/core/src/decisions/panes/uptime.rs"
));

fn authority_service() -> Service {
    parse_manifest(SERVICES_JSON)
        .expect("services.json parses")
        .into_iter()
        .find(|s| s.id == "authority")
        .expect("authority is declared")
}

#[test]
fn the_body_this_poller_writes_is_the_body_the_pane_parses() {
    let body = ProbeBody::from_outcome(&authority_service(), &Outcome::Reached(401));
    let payload = body.envelope();
    let object = payload.get("body").expect("the envelope carries a body").as_object().expect("an object");

    for key in KEYS_THE_PANE_READS {
        assert!(
            object.contains_key(*key),
            "`{key}` is gone from the body this poller writes — `parse_uptime_body` \
             would answer a gap and the pane would read 'no answer yet' forever"
        );
        assert!(
            UPTIME_RS.contains(&format!("object.get(\"{key}\")")),
            "`object.get(\"{key}\")` is gone from uptime.rs's parser — `{key}` is \
             written here but no longer read there, one side was renamed alone"
        );
    }
}

/// The source string, checked against the TypeScript's own literal — the
/// pane refuses anything else against the envelope's `schema`.
#[test]
fn the_source_agrees_with_the_pane() {
    assert!(
        UPTIME_RS.contains(r#"pub const SOURCE: &str = "uptime/v1";"#),
        "the pane reads a different source than this poller writes"
    );
    assert_eq!(hummingbird_domain::UPTIME_V1, "uptime/v1");
}

/// `crontab`'s entry for this binary must agree with this — the declared
/// cadence `Freshness` reads. #792 moved this poller off Actions
/// `schedule:` onto the sweeper's supercronic clock, so the file pinned
/// here is `crontab`, not `.github/workflows/uptime-probe.yml`.
///
/// **Checked against the crontab, not only against itself.** A bare
/// `assert_eq!(POLLED_EVERY_MS, 60 * 60 * 1000)` restates the constant and
/// would still pass the day someone changed the crontab entry and left this
/// alone, which is the exact drift this file exists to catch on every
/// *other* contract it guards. `hummingbird-github-status::cron::
/// declared_cadence_ms` would decide this properly, but reaching it means a
/// cross-crate dependency for a test, so the entry's own minute field is
/// read here instead — no dependency, and the failure still lands on a real
/// edit to `crontab`. The reading is `gmail-poll/tests/contract.rs`'s
/// `firings_per_hour`, verbatim.
const CRONTAB: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crontab"));

/// The one `crontab` line that fires this binary, minute field only — how
/// many distinct minutes it fires within the hour is the cadence, whatever
/// minute was actually picked to stagger it off the other six jobs.
fn firings_per_hour(binary: &str) -> usize {
    let line = CRONTAB
        .lines()
        .find(|l| l.contains(binary))
        .unwrap_or_else(|| panic!("crontab carries no entry for {binary}"));
    // The whole file, not just the first match: a *second* entry for the same
    // binary would double the real cadence while this gate stayed green, and
    // a second clock for one job is the banned failure (CLAUDE.md's "No
    // competing clocks"; issue #8).
    let entries = CRONTAB.lines().filter(|l| l.contains(binary)).count();
    assert_eq!(entries, 1, "crontab declares more than one entry for {binary}; POLLED_EVERY_MS names one cadence");
    let minute_field = line.split_whitespace().next().expect("a minute field");
    minute_field.split(',').count()
}

#[test]
fn polled_every_ms_is_one_hour_and_the_crontab_says_so_too() {
    const MS_PER_HOUR: i64 = 60 * 60 * 1000;
    assert_eq!(POLLED_EVERY_MS, 60 * 60 * 1000);
    assert_eq!(
        MS_PER_HOUR % POLLED_EVERY_MS,
        0,
        "POLLED_EVERY_MS must divide an hour evenly for this gate to read a minute count as a cadence"
    );
    let expected_firings_per_hour = (MS_PER_HOUR / POLLED_EVERY_MS) as usize;
    assert_eq!(
        firings_per_hour("/app/bin/uptime-probe"),
        expected_firings_per_hour,
        "crontab's entry for uptime-probe no longer fires hourly — `POLLED_EVERY_MS` is now a lie, and \
         every pane reading `declaredCadenceMs` bands its freshness against the wrong cadence"
    );
}

/// `HB_INGEST_TOKEN` is the singular env var name every binary on this
/// machine reads (`main.rs`'s own `env("HB_INGEST_TOKEN")`); the per-source
/// secret is mapped onto it on the command line, not read directly.
#[test]
fn the_crontab_entry_maps_the_uptime_probe_ingest_secret_onto_hb_ingest_token() {
    let line = CRONTAB
        .lines()
        .find(|l| l.contains("/app/bin/uptime-probe"))
        .expect("crontab carries an entry for uptime-probe");
    assert!(
        line.contains(r#"HB_INGEST_TOKEN="$UPTIME_PROBE_INGEST_TOKEN""#),
        "uptime-probe's crontab entry no longer maps UPTIME_PROBE_INGEST_TOKEN onto HB_INGEST_TOKEN"
    );
}
