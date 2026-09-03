//! **The cadence drift gate, both binaries.** #774 moved both
//! `graph-mail-poll` and `graph-calendar-poll` off Actions `schedule:` onto
//! the sweeper's own `crontab` — `gmail-poll`'s own `tests/contract.rs`
//! establishes the pattern this file follows verbatim, itself following
//! `server/uptime-probe/tests/contract.rs`'s reasoning: a bare
//! `assert_eq!(POLLED_EVERY_MS, 15 * 60 * 1000)` restates the constant and
//! would still pass the day someone changed the `crontab` entry and left
//! this one alone.

use hummingbird_graph_poll::{CALENDAR_POLLED_EVERY_MS, MAIL_POLLED_EVERY_MS};

const CRONTAB: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crontab"));

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

fn assert_matches_crontab_cadence(binary: &str, polled_every_ms: i64) {
    const MS_PER_HOUR: i64 = 60 * 60 * 1000;
    assert_eq!(
        MS_PER_HOUR % polled_every_ms,
        0,
        "POLLED_EVERY_MS must divide an hour evenly for this gate to read a minute count as a cadence"
    );
    let expected_firings_per_hour = (MS_PER_HOUR / polled_every_ms) as usize;
    assert_eq!(
        firings_per_hour(binary),
        expected_firings_per_hour,
        "crontab's entry for {binary} no longer fires every {polled_every_ms}ms — \
         POLLED_EVERY_MS is now a lie"
    );
}

#[test]
fn mail_polled_every_ms_matches_the_crontab_entrys_cadence() {
    assert_matches_crontab_cadence("/app/bin/graph-mail-poll", MAIL_POLLED_EVERY_MS);
}

#[test]
fn calendar_polled_every_ms_matches_the_crontab_entrys_cadence() {
    assert_matches_crontab_cadence("/app/bin/graph-calendar-poll", CALENDAR_POLLED_EVERY_MS);
}

#[test]
fn the_crontab_entries_map_the_m365_ingest_secrets_onto_hb_ingest_token() {
    let mail_line = CRONTAB
        .lines()
        .find(|l| l.contains("/app/bin/graph-mail-poll"))
        .expect("crontab carries an entry for graph-mail-poll");
    assert!(
        mail_line.contains(r#"HB_INGEST_TOKEN="$M365_MAIL_INGEST_TOKEN""#),
        "graph-mail-poll's crontab entry no longer maps M365_MAIL_INGEST_TOKEN onto HB_INGEST_TOKEN"
    );

    let calendar_line = CRONTAB
        .lines()
        .find(|l| l.contains("/app/bin/graph-calendar-poll"))
        .expect("crontab carries an entry for graph-calendar-poll");
    assert!(
        calendar_line.contains(r#"HB_INGEST_TOKEN="$M365_CALENDAR_INGEST_TOKEN""#),
        "graph-calendar-poll's crontab entry no longer maps M365_CALENDAR_INGEST_TOKEN onto HB_INGEST_TOKEN"
    );
}
