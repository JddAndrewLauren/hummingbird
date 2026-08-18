//! The **zone bridge** (#533/M4) — how a core that owns no tzdb answers a
//! civil-date question about somebody else's time zone.
//!
//! # Why a bridge rather than a zone table
//!
//! A bin collection happens on a day at an address, not at an instant on a
//! device: "tonight" must flip at the address's midnight, not the reader's.
//! So every pane rule that touches a day needs `(zone, instant) ->
//! civil date` and `(zone, civil date) -> instant`, and this crate has no
//! table to answer either with — deliberately, and at a measured price
//! (`client/core/Cargo.toml`'s `chrono-tz` note: the table took the release
//! wasm from 525 KB to 1.41 MB).
//!
//! Both hosts already own one. The web has `Intl.DateTimeFormat`
//! (`waste-pane/zoned-day.ts`, which becomes the web's *resolver* for this
//! bridge and is otherwise unchanged); Android has `java.time.ZoneId`. So
//! the crossing is **two-phase**: the core names every `(zone, civil-date)`
//! fact it needs ([`ZoneQuery`]), the host resolves them into a
//! [`ZoneFacts`] table, and the core ranks against that table. The core
//! keeps every decision; the host contributes one lookup and no judgement.
//!
//! # The unresolvable zone is an absence, and the absence is decided here
//!
//! A zone the host cannot resolve crosses back **absent** — the host simply
//! omits that query's key. It does not send a null, an empty string, or a
//! "bad zone" flag, because any of those would be the host deciding what an
//! unusable zone means. Every reader goes through
//! [`ZoneFacts::civil_date`]/[`ZoneFacts::midnight_ms`], which answer
//! `None`, and the rule that turns `None` into a gap
//! ([`super::waste::WasteGap::UnresolvableZone`]) is a core decision like
//! every other one. There is no accessor that fabricates a fallback, which
//! is what stops a call site inventing one.
//!
//! # What is *not* a zone question
//!
//! Calendar arithmetic over `YYYY-MM-DD` strings needs no tzdb at all, so
//! it lives here in Rust rather than crossing: [`is_civil_date`],
//! [`civil_days_between`], [`add_civil_days`], [`weekday_index`]. That
//! mirrors `zoned-day.ts`'s calendar half while its `Intl` half stays TS.
//! [`weekday_index`] is deliberately an *index*, not a word: which day it
//! is, is a decision; what that day is called is a rendering (and the app
//! has no i18n layer to route a word through anyway).

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

/// A whole civil day, `YYYY-MM-DD` — never an instant, which drifts a day
/// at a zone boundary.
pub type CivilDate = String;

/// Whether `value` is a well-formed `YYYY-MM-DD` **and a real date**.
/// Stricter than `zoned-day.ts`'s regex on purpose: the TS half rejects
/// `"next tuesday"` and accepts `"2026-02-31"`, whose every downstream
/// answer is then `null`; here the same input is refused once, at the door.
pub fn is_civil_date(value: &str) -> bool {
    parse_civil(value).is_some()
}

fn parse_civil(value: &str) -> Option<NaiveDate> {
    // `%Y-%m-%d` alone would accept `2026-8-7`; the length check pins the
    // zero-padded shape the wire actually carries.
    if value.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

/// `date` shifted by whole civil days — calendar arithmetic, never
/// `instant ± n × 86_400_000`, which lands on the wrong civil date across a
/// DST boundary (the `endMs - DAY` defect ADR-0015 records on #121).
pub fn add_civil_days(date: &str, days: i64) -> Option<CivilDate> {
    let base = parse_civil(date)?;
    base.checked_add_signed(Duration::days(days)).map(|d| d.format("%Y-%m-%d").to_string())
}

/// Whole civil days from `from` to `to` — deliberately *not* a subtraction
/// of two instants: the gap between two midnights is 23 or 25 hours across
/// a DST transition, and "three days away" must stay three days through
/// one.
pub fn civil_days_between(from: &str, to: &str) -> Option<i64> {
    let a = parse_civil(from)?;
    let b = parse_civil(to)?;
    Some((b - a).num_days())
}

/// Which day of the week `date` falls on, **`0` = Sunday**, matching the
/// `Date.getDay()`/`java.time` conventions a host maps to a word with a
/// seven-element array. The word itself is per-client (`weekdayInZone` on
/// the web); no locale, no tzdb and no clock is involved in the index.
pub fn weekday_index(date: &str) -> Option<u8> {
    parse_civil(date).map(|d| d.weekday().num_days_from_sunday() as u8)
}

// ------------------------------------------------------------------ bridge

/// One `(zone, civil-date)` fact the core needs and cannot answer itself.
///
/// [`ZoneQuery::key`] is the whole protocol: the core asks by key, the host
/// answers by key, and neither side has to agree on list order or on how
/// many times the same fact was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ZoneQuery {
    /// What civil date `at_ms` falls on in `zone`.
    CivilDate { zone: String, at_ms: i64 },
    /// The instant `date` begins at in `zone` — midnight, at the address.
    Midnight { zone: String, date: CivilDate },
}

impl ZoneQuery {
    /// This query's stable identity in a [`ZoneFacts`] table. Deterministic
    /// and derived only from the question, so asking twice costs one entry
    /// and a host may resolve a deduplicated list.
    pub fn key(&self) -> String {
        match self {
            ZoneQuery::CivilDate { zone, at_ms } => format!("civil:{zone}:{at_ms}"),
            ZoneQuery::Midnight { zone, date } => format!("midnight:{zone}:{date}"),
        }
    }
}

/// One resolved fact. Untagged because the two arms are unambiguous on the
/// wire (a JSON string is a civil date, a JSON number is an instant) and a
/// host that had to tag them would be one more place to get the tag wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ZoneFact {
    Date(CivilDate),
    Instant(i64),
}

/// Everything the host could resolve, keyed by [`ZoneQuery::key`].
///
/// **A key the host omits is the unresolvable zone.** There is no third
/// state and no accessor returning a fallback — see the module header.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ZoneFacts(HashMap<String, ZoneFact>);

impl ZoneFacts {
    /// What civil date `at_ms` falls on in `zone`, or `None` when the host
    /// could not say. A resolved-but-malformed answer is also `None`: the
    /// host is a lookup, not an authority on shape.
    pub fn civil_date(&self, zone: &str, at_ms: i64) -> Option<CivilDate> {
        match self.0.get(&ZoneQuery::CivilDate { zone: zone.to_string(), at_ms }.key()) {
            Some(ZoneFact::Date(date)) if is_civil_date(date) => Some(date.clone()),
            _ => None,
        }
    }

    /// The instant `date` begins at in `zone`, or `None` when the host
    /// could not say.
    pub fn midnight_ms(&self, zone: &str, date: &str) -> Option<i64> {
        match self.0.get(&ZoneQuery::Midnight { zone: zone.to_string(), date: date.to_string() }.key())
        {
            Some(ZoneFact::Instant(ms)) => Some(*ms),
            _ => None,
        }
    }

    /// Record one resolved fact — the constructor a test (or a native host
    /// that resolves in-process) builds a table with.
    pub fn insert(&mut self, query: &ZoneQuery, fact: ZoneFact) {
        self.0.insert(query.key(), fact);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_civil_date_is_a_real_zero_padded_day_and_nothing_else() {
        assert!(is_civil_date("2026-08-17"));
        assert!(!is_civil_date("next tuesday"));
        assert!(!is_civil_date("2026-8-17"));
        assert!(!is_civil_date("2026-02-31"));
        assert!(!is_civil_date(""));
    }

    #[test]
    fn civil_days_between_counts_days_not_milliseconds() {
        assert_eq!(civil_days_between("2026-08-10", "2026-08-17"), Some(7));
        assert_eq!(civil_days_between("2026-08-10", "2026-08-09"), Some(-1));
        // Across the US spring-forward transition, which is 23 hours long:
        // an instant subtraction would answer 0 here.
        assert_eq!(civil_days_between("2026-03-08", "2026-03-09"), Some(1));
        assert_eq!(civil_days_between("2026-08-10", "nope"), None);
    }

    #[test]
    fn add_civil_days_is_calendar_arithmetic_over_a_month_boundary() {
        assert_eq!(add_civil_days("2026-08-31", 1).as_deref(), Some("2026-09-01"));
        assert_eq!(add_civil_days("2026-03-01", -1).as_deref(), Some("2026-02-28"));
        assert_eq!(add_civil_days("nope", 1), None);
    }

    #[test]
    fn weekday_index_counts_from_sunday() {
        // 2026-08-16 is a Sunday; 2026-08-17 the Monday after it.
        assert_eq!(weekday_index("2026-08-16"), Some(0));
        assert_eq!(weekday_index("2026-08-17"), Some(1));
        assert_eq!(weekday_index("2026-08-22"), Some(6));
        assert_eq!(weekday_index("nope"), None);
    }

    #[test]
    fn a_query_key_is_deterministic_and_distinguishes_the_two_kinds() {
        let civil = ZoneQuery::CivilDate { zone: "America/Los_Angeles".into(), at_ms: 17 };
        let midnight =
            ZoneQuery::Midnight { zone: "America/Los_Angeles".into(), date: "2026-08-17".into() };
        assert_eq!(civil.key(), "civil:America/Los_Angeles:17");
        assert_eq!(midnight.key(), "midnight:America/Los_Angeles:2026-08-17");
        assert_eq!(civil.key(), civil.clone().key());
        assert_ne!(civil.key(), midnight.key());
    }

    #[test]
    fn an_omitted_key_reads_as_absent_and_never_as_a_fallback() {
        let facts = ZoneFacts::default();
        assert_eq!(facts.civil_date("Mars/Olympus_Mons", 0), None);
        assert_eq!(facts.midnight_ms("Mars/Olympus_Mons", "2026-08-17"), None);
    }

    #[test]
    fn a_resolved_fact_is_read_back_by_its_own_query() {
        let civil = ZoneQuery::CivilDate { zone: "Europe/London".into(), at_ms: 1_000 };
        let midnight = ZoneQuery::Midnight { zone: "Europe/London".into(), date: "2026-08-17".into() };
        let mut facts = ZoneFacts::default();
        facts.insert(&civil, ZoneFact::Date("2026-08-17".into()));
        facts.insert(&midnight, ZoneFact::Instant(1_755_385_200_000));

        assert_eq!(facts.civil_date("Europe/London", 1_000).as_deref(), Some("2026-08-17"));
        assert_eq!(facts.midnight_ms("Europe/London", "2026-08-17"), Some(1_755_385_200_000));
        // A fact resolved for one zone says nothing about another.
        assert_eq!(facts.civil_date("Europe/Paris", 1_000), None);
    }

    #[test]
    fn a_fact_of_the_wrong_shape_reads_as_absent_rather_than_as_an_answer() {
        let query = ZoneQuery::CivilDate { zone: "Europe/London".into(), at_ms: 1_000 };
        let mut facts = ZoneFacts::default();
        facts.insert(&query, ZoneFact::Date("not a day".into()));
        assert_eq!(facts.civil_date("Europe/London", 1_000), None);
    }

    #[test]
    fn the_facts_table_crosses_as_a_plain_key_to_value_map() {
        let json = r#"{"civil:Europe/London:0":"2026-08-17","midnight:Europe/London:2026-08-17":17}"#;
        let facts: ZoneFacts = serde_json::from_str(json).unwrap();
        assert_eq!(facts.civil_date("Europe/London", 0).as_deref(), Some("2026-08-17"));
        assert_eq!(facts.midnight_ms("Europe/London", "2026-08-17"), Some(17));
    }
}
