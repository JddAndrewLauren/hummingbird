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
///
/// **It takes no clock, and that is load-bearing rather than tidy.** The
/// words this returns become the alert's `title` and `body`, and the
/// authority decides `restamp_on_change` by asking whether a re-raise
/// changed a source-owned field. So *anything* clock-dependent in these
/// strings makes every daily re-poll of an unchanged slide a change, which
/// restamps `raised_at`, which — since `is_live` compares it against
/// `dismissed_at` — undoes the human's dismissal every single morning. That
/// is the exact failure the whole design exists to prevent, and it arrives
/// through the most innocuous-looking field there is.
///
/// An earlier revision interpolated a relative phrase ("in 4 days",
/// "tomorrow") and did precisely that; the prototype carried the same shape.
/// Dropping the `today` parameter is what makes the bug unwritable rather
/// than merely absent: a function with no clock cannot produce a
/// clock-dependent string. **Do not add one back.** How far away the
/// collection is is read-time urgency, computed where every other read-time
/// fact in this system is computed (ADR-0002) — on the pane, from the
/// snapshot, never written into a stored row.
pub fn plan(cadence: Cadence, deviation: Deviation) -> Option<AlertPlan> {
    match deviation {
        Deviation::OnCadence => None,
        Deviation::Slide { scheduled, slides_to } => Some(AlertPlan {
            scheduled,
            affected: if slides_to > scheduled { slides_to } else { scheduled },
            // The title states the change itself, never that one happened —
            // and names the date, so two consecutive holiday weeks read
            // differently on a lock screen as well as in the key.
            title: format!(
                "Collection moves from {} to {} ({slides_to})",
                scheduled.weekday(),
                slides_to.weekday(),
            ),
            body: format!(
                "Collection is normally {}. This cycle moves from {scheduled} to {slides_to}.",
                cadence.describe()
            ),
        }),
        Deviation::SkippedCycle { missed, next_seen } => Some(AlertPlan {
            scheduled: missed,
            affected: missed,
            title: format!("No collection on {} ({missed})", missed.weekday()),
            body: format!(
                "The council's page shows no pickup for {missed}; the next collection it lists \
                 is {next_seen}."
            ),
        }),
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
                plan(c, judge(c, collected_on, today)),
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
            let plan = plan(c, judge(c, collected_on, today)).expect("a slide rings");
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

    /// **The regression test, and the property whose absence let the bug
    /// through.** Every daily re-poll of one unchanged slide must produce a
    /// *byte-identical* wire payload — not just the same key, the same
    /// everything — because the authority decides whether to restamp
    /// `raised_at` by diffing the re-raise against the stored row. One
    /// clock-dependent character anywhere in this payload and the daily poll
    /// silently becomes a daily re-ring over the reader's dismissal.
    ///
    /// The suite already had "the same `source_key` every day" and "a
    /// dismissal survives four re-polls" (with a hand-written fixed title,
    /// authority-side) — and neither could see a title that moved. Compare
    /// the whole payload, not the identity.
    #[test]
    fn a_week_of_re_polls_of_one_unchanged_slide_is_byte_identical_every_day() {
        let c = weekly();
        let collected_on = d("2026-08-18");
        let payloads: std::collections::BTreeSet<String> = (0..=7)
            .map(|offset| {
                let today = d("2026-08-11").add_days(offset);
                let ingest = plan(c, judge(c, collected_on, today))
                    .expect("a slide rings")
                    .ingest("America/Los_Angeles")
                    .expect("a real zone");
                serde_json::to_string(&ingest).expect("the wire payload serializes")
            })
            .collect();
        assert_eq!(
            payloads.len(),
            1,
            "eight polls of one unchanged slide produced {} distinct payloads; \
             every one after the first is a write, and with `restamp_on_change` \
             a write is a fresh `raised_at` over the reader's dismissal:\n{}",
            payloads.len(),
            payloads.iter().cloned().collect::<Vec<_>>().join("\n")
        );
    }

    /// A correction lands on the same row — that is what keying on the
    /// scheduled date buys — while changing the words, which is what earns
    /// the server-side restamp.
    #[test]
    fn a_correction_keeps_the_key_and_changes_the_words() {
        let c = weekly();
        let today = d("2026-08-12");
        let first = plan(c, judge(c, d("2026-08-18"), today)).unwrap();
        let corrected = plan(c, judge(c, d("2026-08-19"), today)).unwrap();
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
        let plan = plan(c, judge(c, d("2026-08-18"), today)).unwrap();
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
        let plan = plan(c, judge(c, d("2026-08-15"), today)).unwrap();
        assert_eq!(plan.scheduled, d("2026-08-17"));
        assert_eq!(plan.affected, d("2026-08-17"), "the later of the two");
    }

    #[test]
    fn a_skipped_cycle_is_loud() {
        let c = weekly();
        let today = d("2026-08-18");
        let plan = plan(c, judge(c, d("2026-08-31"), today)).expect("a skip rings");
        assert_eq!(plan.source_key(), "2026-08-24");
        assert!(plan.title.starts_with("No collection on Monday"), "{}", plan.title);
    }

    /// The poller never sends `raised_at` — it asks the server to decide,
    /// and sending both is a 400.
    #[test]
    fn the_ingest_asks_the_server_to_stamp_and_never_stamps_itself() {
        let c = weekly();
        let today = d("2026-08-12");
        let ingest = plan(c, judge(c, d("2026-08-18"), today))
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
            plan(c, judge(c, d("2026-08-18"), today)).unwrap().ingest("Mars/Olympus"),
            None
        );
    }
}
