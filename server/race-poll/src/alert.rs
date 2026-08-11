//! The alert half: what a race inside the lead time would post to
//! `POST /api/alerts`.
//!
//! Returned rather than performed, so the decision is testable and the write
//! stays in one place (`race_alert_poll.rs`).
//!
//! # The trap this lane inherits from the waste lane
//!
//! **[`plan`] takes no clock, and that is load-bearing rather than tidy.**
//! The words it returns become the alert's `title` and `body`, and the
//! authority decides `restamp_on_change` by asking whether a re-raise
//! changed a source-owned field. So anything clock-dependent in these
//! strings ("in 42 minutes", "tonight") makes every one of the ~6 re-posts
//! inside one 90-minute window a *change*, which restamps `raised_at`, which
//! — since `is_live` compares it against `dismissed_at` — undoes the
//! reader's dismissal on the very next poll.
//!
//! A race pane whose whole subject is a countdown is the most tempting
//! possible place to write one, and `server/city-waste/src/alert.rs` records
//! that both its prototype and its first revision did. Taking no `now`
//! parameter is what makes the bug unwritable rather than merely absent:
//! a function with no clock cannot produce a clock-dependent string. **Do
//! not add one back.** The `90` in the body is formatted from
//! [`crate::next::LEAD_MS`], which is a constant, never from a distance to
//! the race. How far away the race is is read-time urgency, computed where
//! every other read-time fact is (ADR-0002) — on the pane, from the
//! snapshot, never written into a stored row.
//!
//! `re_posting_the_same_race_alert_is_byte_identical_at_every_lead_offset`
//! is the only thing that actually catches a relative phrase creeping back
//! in — and it compares the whole serialized payload, not the `source_key`,
//! because "the same key every poll" is exactly the assertion that cannot
//! see a title that moved.

use hummingbird_domain::{race_schedule_v1_key, AlertIngest, RACE_SCHEDULE_V1};

use crate::body::RaceEvent;
use crate::next::LEAD_MS;

/// Everything one occurrence needs, before it becomes an `AlertIngest`.
/// Separated so a test can read the decision — which instant is the key,
/// which words go on the lock screen — without going through serde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertPlan {
    /// The series key, which is both the `context_snapshots.key` this alert
    /// is about and the first half of the occurrence key.
    pub series: String,
    /// The race start: the occurrence's identity, and its expiry.
    pub starts_at_ms: i64,
    pub title: String,
    pub body: String,
}

impl AlertPlan {
    pub fn source_key(&self) -> String {
        race_schedule_v1_key(&self.series, self.starts_at_ms)
    }

    /// The wire body.
    pub fn ingest(&self) -> AlertIngest {
        AlertIngest {
            source: RACE_SCHEDULE_V1.to_string(),
            source_key: self.source_key(),
            // The pane join's own half: `(source, subject_key)` ↔
            // `(source, key)`, and the snapshot's key is the series.
            subject_key: Some(self.series.clone()),
            title: self.title.clone(),
            body: Some(self.body.clone()),
            // Nothing useful to link once the feed's wikipedia `url` is
            // dropped from the body — a link to an encyclopedia article is
            // not what a get-to-the-couch nudge is for.
            url: None,
            // ADR-0012's urgent tier means high transport priority and a
            // different Android channel. A race is a nudge, not an
            // emergency.
            severity: Some("normal".to_string()),
            // Never sent: `restamp_on_change` owns this field, and sending
            // both is a 400.
            raised_at: None,
            resolved_at: None,
            // ADR-0014's `Expiry::Always("the race's start time")`: the
            // occurrence ends when the race begins, with no human action.
            expires_at: Some(self.starts_at_ms),
            // Matching `city-waste`. Within one occurrence nothing
            // source-owned changes, so the ~6 re-posts inside one window
            // never restamp and a dismissal survives; a corrected race name
            // still rings.
            restamp_on_change: true,
        }
    }
}

/// What to post about one race that is inside the lead time. Takes the
/// series (the row key it was read from) and the race — and **no clock**;
/// see the module header before adding one.
pub fn plan(series: &str, event: &RaceEvent) -> AlertPlan {
    AlertPlan {
        series: series.to_string(),
        starts_at_ms: event.starts_at_ms,
        // No series prefix: race names are already unambiguous.
        title: format!("{} starts soon", event.name),
        // The `90` is formatted from the constant, never from a distance to
        // the race. "about" is carrying the ±15 minutes the alert cron
        // introduces.
        body: format!("Race start in about {} minutes.", LEAD_MS / 60_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::next::race_within_lead;

    const RACE: i64 = 1_772_942_400_000;

    fn australian_grand_prix() -> RaceEvent {
        RaceEvent {
            name: "Australian Grand Prix".to_string(),
            locality: "Melbourne".to_string(),
            starts_at_ms: RACE,
            sessions: vec![],
        }
    }

    /// The words, pinned. Both halves are what someone reads on a lock
    /// screen with no app open, so they are worth stating literally.
    #[test]
    fn the_alert_names_the_race_and_the_lead_time() {
        let plan = plan("f1", &australian_grand_prix());
        assert_eq!(plan.title, "Australian Grand Prix starts soon");
        assert_eq!(plan.body, "Race start in about 90 minutes.");
        assert_eq!(plan.source_key(), "f1:1772942400000");
    }

    /// **The regression test, and the property whose absence would let the
    /// bug through.** Every re-post inside one 90-minute window must produce
    /// a *byte-identical* wire payload — not just the same key, the same
    /// everything — because the authority decides whether to restamp
    /// `raised_at` by diffing the re-raise against the stored row. One
    /// clock-dependent character anywhere in this payload and the
    /// fifteen-minute poll silently becomes a re-ring over the reader's
    /// dismissal, six times per race.
    #[test]
    fn re_posting_the_same_race_alert_is_byte_identical_at_every_lead_offset() {
        let season = vec![australian_grand_prix()];
        let payloads: std::collections::BTreeSet<String> = (0..=LEAD_MS / (15 * 60 * 1000))
            .map(|slot| {
                let now = RACE - LEAD_MS + slot * 15 * 60 * 1000;
                let event = race_within_lead(&season, now, LEAD_MS)
                    .unwrap_or_else(|| panic!("the race is inside the window at {now}"));
                serde_json::to_string(&plan("f1", event).ingest())
                    .expect("the wire payload serializes")
            })
            .collect();
        assert_eq!(
            payloads.len(),
            1,
            "seven polls of one race produced {} distinct payloads; every one \
             after the first is a write, and with `restamp_on_change` a write \
             is a fresh `raised_at` over the reader's dismissal:\n{}",
            payloads.len(),
            payloads.iter().cloned().collect::<Vec<_>>().join("\n")
        );
    }

    /// The wire, checked as **JSON** rather than as a struct.
    /// `restamp_on_change` carries a `skip_serializing_if`, so a wrong
    /// predicate would drop it silently and every re-post would stop asking
    /// the server to decide the stamp — the dismissal-undoing bug,
    /// reintroduced by a serde attribute and invisible to any assertion on
    /// the struct.
    #[test]
    fn the_ingest_asks_the_server_to_stamp_and_never_stamps_itself() {
        let wire = serde_json::to_value(plan("f1", &australian_grand_prix()).ingest()).unwrap();
        assert_eq!(wire["restamp_on_change"], serde_json::json!(true));
        assert_eq!(wire["source"], "race-schedule/v1");
        assert_eq!(wire["source_key"], "f1:1772942400000");
        assert_eq!(wire["subject_key"], "f1", "the pane join's own half");
        assert_eq!(wire["severity"], "normal");
        assert_eq!(wire["expires_at"], serde_json::json!(RACE));
        assert!(wire.get("raised_at").is_none(), "sending both is a 400");
        assert!(wire.get("url").is_none());
    }

    /// A **postponed** race is a new occurrence and rings; the alert left
    /// behind expires at its own, now past, start time and needs no
    /// cleanup.
    #[test]
    fn a_postponed_race_is_a_new_occurrence_that_rings() {
        let published = plan("f1", &australian_grand_prix());
        let mut moved = australian_grand_prix();
        moved.starts_at_ms = RACE + 28 * 24 * 60 * 60 * 1000;
        let postponed = plan("f1", &moved);

        assert_ne!(published.source_key(), postponed.source_key());
        assert_eq!(published.title, postponed.title, "same race, same words");
        assert_eq!(
            published.ingest().expires_at,
            Some(RACE),
            "the abandoned occurrence expires at the start it no longer has"
        );
    }

    /// A **corrected race name** lands on the row already minted for that
    /// start and changes the words, which is what earns the server-side
    /// restamp — the other half of `restamp_on_change`'s bargain.
    #[test]
    fn a_corrected_race_name_keeps_the_key_and_changes_the_words() {
        let first = plan("f1", &australian_grand_prix());
        let mut corrected = australian_grand_prix();
        corrected.name = "Australian Grand Prix (Melbourne)".to_string();
        let corrected = plan("f1", &corrected);
        assert_eq!(first.source_key(), corrected.source_key(), "one occurrence");
        assert_ne!(first.title, corrected.title, "and new information in it");
    }

    /// Two series' races at the same instant are two occurrences and two
    /// pane subjects — the series reaches both halves of the identity.
    #[test]
    fn the_series_reaches_both_the_occurrence_key_and_the_pane_join() {
        let f1 = plan("f1", &australian_grand_prix()).ingest();
        let indycar = plan("indycar", &australian_grand_prix()).ingest();
        assert_ne!(f1.source_key, indycar.source_key);
        assert_eq!(f1.subject_key.as_deref(), Some("f1"));
        assert_eq!(indycar.subject_key.as_deref(), Some("indycar"));
    }
}
