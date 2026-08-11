//! The alert half: what a [`Deviation`] would post to `POST /api/alerts`.
//!
//! Returned rather than performed, so the decision is testable and the write
//! stays in one place (`main.rs`). This is `waste-cadence`'s `mint`, with the
//! `restamp_raised_at` question **removed** — that decision moved to the
//! server as `AlertIngest::restamp_on_change` (#120), because a poller
//! holding an ingest token cannot read the alert back and so cannot tell an
//! unchanged re-poll from a correction. Here it is simply always requested;
//! the authority decides what it means on the day.

use hummingbird_domain::{city_waste_v2_key, AlertIngest, CITY_WASTE_V2};

use crate::body::SNAPSHOT_KEY;
use crate::cadence::Cadence;
use crate::date::Date;
use crate::judge::Deviation;

/// Everything one occurrence needs, before it becomes an `AlertIngest`.
/// Separated so the tests can read the decision (which date is the key?
/// which date does it expire on?) without going through serde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertPlan {
    /// The originally scheduled collection date — the fixed coordinate the
    /// occurrence key is built from.
    pub scheduled: Date,
    /// End of the **later** of the scheduled and the slid-to date. Read as
    /// the scheduled Monday, the holiday text would vanish from the pane on
    /// the Tuesday morning it exists to warn about.
    pub affected: Date,
    pub title: String,
    pub body: String,
}

impl AlertPlan {
    pub fn source_key(&self) -> String {
        city_waste_v2_key(&self.scheduled.iso())
    }

    /// The wire body. `zone` resolves `expires_at` at the address's midnight
    /// (see [`Date::end_of_day_ms`]); `None` for a zone the tzdb has never
    /// heard of, which the caller must treat as a malformed page rather than
    /// posting an alert with no expiry.
    pub fn ingest(&self, zone: &str) -> Option<AlertIngest> {
        Some(AlertIngest {
            source: CITY_WASTE_V2.to_string(),
            source_key: self.source_key(),
            // The pane join genuinely holds, and it is additive. `waste.ts`
            // currently never reads `liveAlerts` — a holiday *is* the answer,
            // read off the snapshot — but leaving this null would record a
            // falsehood to match a temporary client choice, and the alert
            // still serves the notification lane either way.
            subject_key: Some(SNAPSHOT_KEY.to_string()),
            title: self.title.clone(),
            body: Some(self.body.clone()),
            url: None,
            severity: Some("normal".to_string()),
            // Never sent: `restamp_on_change` owns this field, and sending
            // both is a 400.
            raised_at: None,
            resolved_at: None,
            expires_at: Some(self.affected.end_of_day_ms(zone)?),
            // The whole reason this poller can be stateless about lifecycle.
            // A daily re-poll of an unchanged slide writes an identical row,
            // which the authority answers as a no-op, so a dismissal stands;
            // a correction changes the title and body, which is what earns a
            // fresh `raised_at` and rings over that dismissal.
            restamp_on_change: true,
        })
    }
}

/// The alert decision for one reading. `None` means there is nothing to say
/// — which is only ever the on-cadence case, i.e. most days.
pub fn plan(cadence: Cadence, deviation: Deviation, today: Date) -> Option<AlertPlan> {
    match deviation {
        Deviation::OnCadence => None,
        Deviation::Slide { scheduled, slides_to } => Some(AlertPlan {
            scheduled,
            affected: if slides_to > scheduled { slides_to } else { scheduled },
            // The title states the change itself, never that one happened.
            title: format!(
                "Collection moves from {} to {} {}",
                scheduled.weekday(),
                slides_to.weekday(),
                when_phrase(scheduled, today)
            ),
            body: format!(
                "Collection is normally {}. This cycle moves from {scheduled} to {slides_to}.",
                cadence.describe()
            ),
        }),
        Deviation::SkippedCycle { missed, next_seen } => Some(AlertPlan {
            scheduled: missed,
            affected: missed,
            title: format!("No {} collection {}", missed.weekday(), when_phrase(missed, today)),
            body: format!(
                "The council's page shows no pickup for {missed}; the next collection it lists \
                 is {next_seen}."
            ),
        }),
    }
}

/// Words for how far off a date is. A seven-day window from today rather
/// than real week boundaries — the prototype flagged this as unfinished and
/// it still is, deliberately: "this week" is a calendar-week question, and
/// answering it needs the address's week start, which the page does not
/// state. The wording is chosen to survive the imprecision ("in N days"
/// beyond a week rather than "the week of").
fn when_phrase(date: Date, today: Date) -> String {
    match date.days() - today.days() {
        d if d < 0 => "(already past)".to_string(),
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        d if d < 7 => format!("in {d} days"),
        d => format!("on {date} ({d} days away)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::judge;

    fn weekly() -> Cadence {
        Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 }
    }

    fn d(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    /// **The property, not a spot check.** Poll every day from a Tuesday
    /// through the next Monday's pickup and the Tuesday after: `plan` is
    /// `None` on every single one. A roll-forward is the largest diff the
    /// page ever shows and it must stay completely silent.
    #[test]
    fn a_roll_forward_across_a_whole_week_raises_nothing() {
        let c = weekly();
        for offset in 0..=8 {
            let today = d("2026-08-11").add_days(offset);
            let collected_on = c.next_on_or_after(today);
            assert_eq!(
                plan(c, judge(c, collected_on, today), today),
                None,
                "polled {today}, page says {collected_on}"
            );
        }
    }

    /// One alert for one holiday: the same `source_key` on every day of the
    /// slide week, and a title that states the change.
    #[test]
    fn a_holiday_slide_raises_exactly_one_alert_naming_the_change() {
        let c = weekly();
        let collected_on = d("2026-08-18");
        let mut keys = std::collections::BTreeSet::new();
        for offset in 0..=7 {
            let today = d("2026-08-11").add_days(offset);
            let plan = plan(c, judge(c, collected_on, today), today).expect("a slide rings");
            keys.insert(plan.source_key());
            assert!(
                plan.title.contains("Monday") && plan.title.contains("Tuesday"),
                "the title states the change: {}",
                plan.title
            );
            assert!(plan.body.contains("2026-08-18"), "{}", plan.body);
        }
        assert_eq!(
            keys.into_iter().collect::<Vec<_>>(),
            vec!["2026-08-17".to_string()],
            "eight polls, one occurrence"
        );
    }

    /// A correction lands on the same row — that is what keying on the
    /// scheduled date buys — while changing the words, which is what earns
    /// the server-side restamp.
    #[test]
    fn a_correction_keeps_the_key_and_changes_the_words() {
        let c = weekly();
        let today = d("2026-08-12");
        let first = plan(c, judge(c, d("2026-08-18"), today), today).unwrap();
        let corrected = plan(c, judge(c, d("2026-08-19"), today), today).unwrap();
        assert_eq!(first.source_key(), corrected.source_key(), "one occurrence");
        assert_ne!(first.title, corrected.title, "and new information in it");
        assert!(corrected.title.contains("Wednesday"), "{}", corrected.title);
    }

    /// ADR-0014's expiry, and the reason the pane can go quiet with no human
    /// action: the alert is live through the slid-to Tuesday and gone by
    /// Wednesday morning. Read as the *scheduled* Monday it would vanish on
    /// the Tuesday morning it exists to warn about.
    #[test]
    fn expiry_is_the_end_of_the_later_date() {
        let c = weekly();
        let today = d("2026-08-12");
        let plan = plan(c, judge(c, d("2026-08-18"), today), today).unwrap();
        assert_eq!(plan.affected, d("2026-08-18"), "the later of the two");

        let zone = "America/Los_Angeles";
        let expires_at = plan.ingest(zone).unwrap().expires_at.unwrap();
        let tuesday_evening = d("2026-08-18").end_of_day_ms(zone).unwrap() - 60 * 60 * 1000;
        let wednesday_morning = d("2026-08-19").end_of_day_ms(zone).unwrap() - 16 * 60 * 60 * 1000;
        assert!(
            hummingbird_domain::is_live(0, None, None, Some(expires_at), tuesday_evening),
            "live on the evening it is about"
        );
        assert!(
            !hummingbird_domain::is_live(0, None, None, Some(expires_at), wednesday_morning),
            "gone by the next morning, with no human action"
        );
    }

    /// A backward slide expires on the date that was *scheduled*, not the
    /// earlier date it moved to — same rule ("the later of the two"),
    /// reached from the other side.
    #[test]
    fn a_backward_slide_expires_on_the_scheduled_date() {
        let c = weekly();
        let today = d("2026-08-12");
        let plan = plan(c, judge(c, d("2026-08-15"), today), today).unwrap();
        assert_eq!(plan.scheduled, d("2026-08-17"));
        assert_eq!(plan.affected, d("2026-08-17"), "the later of the two");
    }

    #[test]
    fn a_skipped_cycle_is_loud() {
        let c = weekly();
        let today = d("2026-08-18");
        let plan = plan(c, judge(c, d("2026-08-31"), today), today).expect("a skip rings");
        assert_eq!(plan.source_key(), "2026-08-24");
        assert!(plan.title.starts_with("No Monday collection"), "{}", plan.title);
    }

    /// The poller never sends `raised_at` — it asks the server to decide,
    /// and sending both is a 400.
    #[test]
    fn the_ingest_asks_the_server_to_stamp_and_never_stamps_itself() {
        let c = weekly();
        let today = d("2026-08-12");
        let ingest = plan(c, judge(c, d("2026-08-18"), today), today)
            .unwrap()
            .ingest("America/Los_Angeles")
            .unwrap();
        assert!(ingest.restamp_on_change);
        assert_eq!(ingest.raised_at, None);
        assert_eq!(ingest.source, "city-waste/v2");
        assert_eq!(ingest.subject_key.as_deref(), Some("collection"));
    }

    /// An unusable zone is refused rather than silently expiring at UTC
    /// midnight — the same rule `parseWasteBody` applies on the client.
    #[test]
    fn an_unknown_zone_yields_no_ingest_at_all() {
        let c = weekly();
        let today = d("2026-08-12");
        assert_eq!(
            plan(c, judge(c, d("2026-08-18"), today), today).unwrap().ingest("Mars/Olympus"),
            None
        );
    }
}
