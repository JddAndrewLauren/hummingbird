//! **Materiality is deviation from cadence, not a diff against the previous
//! snapshot.** [`judge`] never sees the last poll, and that single choice is
//! what makes the whole lane behave:
//!
//! * the ordinary roll-forward is silent — the morning after a pickup the
//!   page starts advertising next week, which is a *large* diff and a *zero*
//!   deviation;
//! * the poller is correct on its very first run, with nothing to compare
//!   against, and correct again after a wiped or lost snapshot;
//! * it needs no state of its own, which is why it can be a one-shot process
//!   fired by cron.
//!
//! The price is that the payload must carry the cadence, not just the next
//! date: without it there is nothing to deviate *from*.

use crate::cadence::Cadence;
use crate::date::Date;

/// How far a collection may move and still be read as a *slide* of the
/// cadence date it is nearest to, in either direction.
///
/// This is the explicit window the backward-slide guard needs. Without one,
/// a pickup moved *earlier* (Monday the 17th collected on Saturday the 15th)
/// resolves against `latest_on_or_before` to the *previous* Monday and reads
/// as a five-day forward slide of a collection that already happened — wrong
/// occurrence key, wrong words, and an alert about the wrong week.
///
/// Three days is the honest bound for a municipal holiday shift. Beyond it,
/// [`judge`] stops calling the reading a slide at all: a collection nowhere
/// near either neighbouring cadence date is a cycle that is not happening as
/// scheduled, and [`Deviation::SkippedCycle`] says so out loud. For a weekly
/// cadence every day is within three of some collection day, so the far arm
/// is reachable only on a longer period — which is exactly where it means
/// something.
pub const MAX_SLIDE_DAYS: i64 = 3;

/// What one reading deviates by. Computed from the reading plus today —
/// deliberately never from the previous snapshot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Deviation {
    /// The observed collection is exactly where the cadence puts it.
    /// Includes the roll-forward.
    OnCadence,
    /// The holiday slide: this cycle's collection moved. `scheduled` is the
    /// fixed coordinate the occurrence key is built from, so a later
    /// correction to `slides_to` lands on the same alert.
    Slide { scheduled: Date, slides_to: Date },
    /// A whole cycle went missing — the observed collection is two or more
    /// periods out, or it is too far from any cadence date to be a slide.
    ///
    /// **Kept even though the corrected domain has no cancelled week.** If
    /// it ever fires it is either a real cancellation or a parse failure,
    /// and both deserve to be loud; deleting the arm would convert them into
    /// the prototype's "loudest possible silence".
    SkippedCycle { missed: Date, next_seen: Date },
}

/// The materiality judgement. `today` is the caller's clock — injected,
/// never read here, the same discipline as the rest of the server.
///
/// `collected_on` is what the page **observes**; `scheduled` is **derived**
/// from the cadence, which is what makes a holiday a derivable reading
/// rather than something the city has to say out loud.
pub fn judge(cadence: Cadence, collected_on: Date, today: Date) -> Deviation {
    // The backward-slide guard, and it is one line rather than a special
    // case: resolve the observed collection against the cadence date it is
    // *nearest* to, not the one behind it. A tie (possible only on an even
    // period) goes to the earlier date, so the reading stays a forward
    // slide, which is the overwhelmingly more common real event.
    let behind = cadence.latest_on_or_before(collected_on);
    let ahead = cadence.next_on_or_after(collected_on);
    let scheduled = if collected_on.days() - behind.days() <= ahead.days() - collected_on.days() {
        behind
    } else {
        ahead
    };

    // Which cycle is this, relative to the one we are currently inside?
    // 0 = this one (possibly slid past today), 1 = the ordinary roll-forward
    // after a completed pickup, 2+ = a cycle went missing.
    let current = cadence.latest_on_or_before(today);
    let cycles_ahead = (scheduled.days() - current.days()) / cadence.period_days();
    if cycles_ahead >= 2 {
        return Deviation::SkippedCycle {
            missed: cadence.next_on_or_after(today),
            next_seen: collected_on,
        };
    }

    let offset = collected_on.days() - scheduled.days();
    if offset == 0 {
        return Deviation::OnCadence;
    }
    if offset.abs() <= MAX_SLIDE_DAYS {
        return Deviation::Slide { scheduled, slides_to: collected_on };
    }
    Deviation::SkippedCycle { missed: scheduled, next_seen: collected_on }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> Cadence {
        Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 }
    }

    fn d(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    /// **The property, not a spot check.** Poll every single day from a
    /// Tuesday through the next Monday's pickup and the Tuesday after it,
    /// with the page advertising whatever the cadence says next — the
    /// roll-forward included, which is the largest diff the page ever shows
    /// and must still be silent.
    #[test]
    fn a_roll_forward_across_a_whole_week_deviates_by_nothing() {
        let c = weekly();
        for offset in 0..=8 {
            let today = d("2026-08-11").add_days(offset);
            let collected_on = c.next_on_or_after(today);
            assert_eq!(
                judge(c, collected_on, today),
                Deviation::OnCadence,
                "polled {today}, page says {collected_on}"
            );
        }
    }

    /// The slide is read off the same cadence every day of the affected
    /// week, and every one of those readings names the same `scheduled`
    /// coordinate — which is what makes them one occurrence.
    #[test]
    fn a_holiday_slide_reads_the_same_scheduled_date_every_day_of_the_week() {
        let c = weekly();
        let collected_on = d("2026-08-18");
        for offset in 0..=7 {
            let today = d("2026-08-11").add_days(offset);
            assert_eq!(
                judge(c, collected_on, today),
                Deviation::Slide { scheduled: d("2026-08-17"), slides_to: collected_on },
                "polled {today}"
            );
        }
    }

    /// The guard the prototype flagged and left open. A pickup moved
    /// *earlier* must resolve against the Monday it moved away from, not
    /// the Monday before that — otherwise it reads as a five-day forward
    /// slide of a collection that already happened.
    #[test]
    fn a_backward_slide_keys_on_the_cadence_date_it_moved_away_from() {
        let c = weekly();
        assert_eq!(
            judge(c, d("2026-08-15"), d("2026-08-12")),
            Deviation::Slide { scheduled: d("2026-08-17"), slides_to: d("2026-08-15") },
            "Saturday the 15th is Monday the 17th arriving early, \
             not Monday the 10th arriving five days late"
        );
    }

    /// The window is a real boundary, tested on both sides. On a fortnightly
    /// cadence a collection halfway through the period is not a slide of
    /// anything — it is a cycle that did not happen where it was supposed to,
    /// and the loud arm is the honest answer.
    #[test]
    fn a_collection_too_far_from_any_cadence_date_is_not_called_a_slide() {
        let c = Cadence { anchor: d("2026-08-03"), every_n_weeks: 2 };
        // Three days out: still a slide.
        assert_eq!(
            judge(c, d("2026-08-20"), d("2026-08-14")),
            Deviation::Slide { scheduled: d("2026-08-17"), slides_to: d("2026-08-20") }
        );
        // Four days out, and four back from the next one: neither.
        assert!(matches!(
            judge(c, d("2026-08-21"), d("2026-08-14")),
            Deviation::SkippedCycle { .. }
        ));
    }

    #[test]
    fn a_page_two_periods_out_is_a_skipped_cycle() {
        let c = weekly();
        // Polled Tuesday the 18th, the page shows the 31st: the 24th is not
        // happening.
        assert_eq!(
            judge(c, d("2026-08-31"), d("2026-08-18")),
            Deviation::SkippedCycle { missed: d("2026-08-24"), next_seen: d("2026-08-31") }
        );
    }

    /// Named rather than papered over: on the collection day itself, "no
    /// pickup today, next is next week" is byte-identical to "pickup
    /// happened, rolled forward". Undetectable from the page alone, by
    /// construction — the skip only becomes visible the following day.
    #[test]
    fn a_skip_is_invisible_on_the_collection_day_itself() {
        let c = weekly();
        assert_eq!(judge(c, d("2026-08-24"), d("2026-08-17")), Deviation::OnCadence);
        assert!(matches!(
            judge(c, d("2026-08-31"), d("2026-08-18")),
            Deviation::SkippedCycle { .. }
        ));
    }

    /// Two consecutive holiday weeks are two occurrences (ADR-0012's
    /// Christmas/New Year case, written about this source), which falls out
    /// of `scheduled` moving with the cycle.
    #[test]
    fn consecutive_holiday_weeks_name_different_scheduled_dates() {
        let c = Cadence { anchor: d("2026-12-07"), every_n_weeks: 1 };
        let first = judge(c, d("2026-12-22"), d("2026-12-18"));
        let second = judge(c, d("2026-12-29"), d("2026-12-25"));
        assert_eq!(
            first,
            Deviation::Slide { scheduled: d("2026-12-21"), slides_to: d("2026-12-22") }
        );
        assert_eq!(
            second,
            Deviation::Slide { scheduled: d("2026-12-28"), slides_to: d("2026-12-29") }
        );
        assert_ne!(first, second);
    }
}
