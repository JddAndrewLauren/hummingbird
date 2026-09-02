//! **The cadence drift gate.** #774 moved this poller off Actions
//! `schedule:` onto the sweeper's own `crontab` — `gmail-poll`'s own
//! `tests/contract.rs` establishes the pattern this file follows verbatim,
//! itself following `server/uptime-probe/tests/contract.rs`'s reasoning: a
//! bare `assert_eq!(POLLED_EVERY_MS, 15 * 60 * 1000)` restates the constant
//! and would still pass the day someone changed the `crontab` entry and
//! left this one alone.

use hummingbird_calendar_poll::POLLED_EVERY_MS;

const CRONTAB: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crontab"));

fn firings_per_hour(binary: &str) -> usize {
    let line = CRONTAB
        .lines()
        .find(|l| l.contains(binary))
        .unwrap_or_else(|| panic!("crontab carries no entry for {binary}"));
    let minute_field = line.split_whitespace().next().expect("a minute field");
    minute_field.split(',').count()
}

#[test]
fn polled_every_ms_matches_the_crontab_entrys_cadence() {
    const MS_PER_HOUR: i64 = 60 * 60 * 1000;
    assert_eq!(
        MS_PER_HOUR % POLLED_EVERY_MS,
        0,
        "POLLED_EVERY_MS must divide an hour evenly for this gate to read a minute count as a cadence"
    );
    let expected_firings_per_hour = (MS_PER_HOUR / POLLED_EVERY_MS) as usize;
    assert_eq!(
        firings_per_hour("/app/bin/hummingbird-calendar-poll"),
        expected_firings_per_hour,
        "crontab's entry for hummingbird-calendar-poll no longer fires every {POLLED_EVERY_MS}ms — \
         POLLED_EVERY_MS is now a lie"
    );
}

#[test]
fn the_crontab_entry_maps_the_calendar_ingest_secret_onto_hb_ingest_token() {
    let line = CRONTAB
        .lines()
        .find(|l| l.contains("/app/bin/hummingbird-calendar-poll"))
        .expect("crontab carries an entry for hummingbird-calendar-poll");
    assert!(
        line.contains(r#"HB_INGEST_TOKEN="$CALENDAR_INGEST_TOKEN""#),
        "hummingbird-calendar-poll's crontab entry no longer maps CALENDAR_INGEST_TOKEN onto HB_INGEST_TOKEN"
    );
}
