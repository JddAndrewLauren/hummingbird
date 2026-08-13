//! Reading a declared cadence off a workflow's own `cron:` string.
//!
//! **Deliberately not a general cron parser.** This repo's own scheduled
//! workflows use exactly three shapes today — `*/N * * * *` (every N
//! minutes), `0 */N * * *` (every N hours, on the hour) and a fixed
//! `MM HH * * *` (once a day) — and [`declared_cadence_ms`] recognises only
//! those three, refusing (returning `None`) rather than guessing at anything
//! else. A workflow whose cadence cannot be read this way still gets a pane
//! (its conclusion is still reported, see `runs.rs`) — it simply cannot
//! be judged overdue against a cadence nothing here could work out, which is
//! the honest reading: a guessed cadence that was wrong would lift or
//! silence a band for the wrong reason.

/// A workflow's `*/N`-shaped field, read as its step. `None` for anything
/// else — a bare `*`, a fixed number, a range, a list.
fn step_of(field: &str) -> Option<u32> {
    let n: u32 = field.strip_prefix("*/")?.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(n)
}

/// A fixed (non-wildcard) field, read as a plain number.
fn fixed_of(field: &str) -> Option<u32> {
    field.parse().ok()
}

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// The declared cadence, in milliseconds, one five-field cron expression
/// names — or `None` if this parser does not recognise the shape.
///
/// Every recognised shape requires day-of-month, month and day-of-week to
/// all be `*`: none of this repo's schedules names any of the three, and a
/// workflow that started doing so would be declaring a cadence this
/// function has no business inferring (e.g. "only on Mondays" is not "every
/// 7 days").
pub fn declared_cadence_ms(cron_expression: &str) -> Option<i64> {
    let fields: Vec<&str> = cron_expression.split_whitespace().collect();
    let [minute, hour, day_of_month, month, day_of_week] = fields.as_slice() else {
        return None;
    };
    if *day_of_month != "*" || *month != "*" || *day_of_week != "*" {
        return None;
    }

    // `*/N * * * *` — every N minutes, whatever the hour.
    if let Some(step) = step_of(minute) {
        return (*hour == "*").then(|| step as i64 * MINUTE_MS);
    }

    // A fixed minute — either `0 */N * * *` (every N hours) or `MM HH * * *`
    // (a fixed time once a day).
    if fixed_of(minute).is_some() {
        if let Some(step) = step_of(hour) {
            return Some(step as i64 * HOUR_MS);
        }
        if fixed_of(hour).is_some() {
            return Some(DAY_MS);
        }
    }

    None
}

/// The smallest (most frequent) cadence among a workflow's `schedule:`
/// entries — a workflow with several cron lines is expected at least as
/// often as its tightest one. `None` if the list is empty or every entry's
/// shape is unrecognised; an entry this parser cannot read is simply
/// dropped from the comparison rather than poisoning the whole workflow's
/// cadence with a guess.
pub fn tightest_cadence_ms(cron_expressions: &[String]) -> Option<i64> {
    cron_expressions
        .iter()
        .filter_map(|c| declared_cadence_ms(c))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fifteen_minutes_is_recognised() {
        // gmail-poll.yml, calendar-poll.yml, graph-mail-poll.yml,
        // graph-calendar-poll.yml, race-alert-poll.yml.
        assert_eq!(declared_cadence_ms("*/15 * * * *"), Some(15 * MINUTE_MS));
    }

    #[test]
    fn every_six_hours_is_recognised() {
        // kimi-balance.yml, race-schedule-poll.yml.
        assert_eq!(declared_cadence_ms("0 */6 * * *"), Some(6 * HOUR_MS));
    }

    #[test]
    fn a_fixed_daily_time_is_recognised() {
        // city-waste.yml.
        assert_eq!(declared_cadence_ms("40 13 * * *"), Some(DAY_MS));
    }

    #[test]
    fn a_bare_wildcard_minute_and_hour_is_unrecognised() {
        // `* * * * *` names no cadence this parser will guess at — "every
        // minute" would be a real reading, but nothing in this repo means
        // that, and a wrong guess here is worse than an honest `None`.
        assert_eq!(declared_cadence_ms("* * * * *"), None);
    }

    #[test]
    fn a_day_of_week_or_month_restriction_is_refused() {
        assert_eq!(declared_cadence_ms("0 9 * * 1"), None, "Mondays only, not every 7 days");
        assert_eq!(declared_cadence_ms("0 0 1 * *"), None, "the 1st of the month, not every 30 days");
        assert_eq!(declared_cadence_ms("0 0 * 12 *"), None, "December only");
    }

    #[test]
    fn a_wrong_field_count_is_refused() {
        assert_eq!(declared_cadence_ms("*/15 * * *"), None);
        assert_eq!(declared_cadence_ms(""), None);
    }

    #[test]
    fn a_zero_step_is_refused_rather_than_treated_as_every_instant() {
        assert_eq!(declared_cadence_ms("*/0 * * * *"), None);
    }

    #[test]
    fn tightest_cadence_picks_the_most_frequent_entry() {
        let crons = vec!["0 */6 * * *".to_string(), "*/15 * * * *".to_string()];
        assert_eq!(tightest_cadence_ms(&crons), Some(15 * MINUTE_MS));
    }

    #[test]
    fn tightest_cadence_skips_unrecognised_entries_rather_than_poisoning_the_result() {
        let crons = vec!["0 9 * * 1".to_string(), "0 */6 * * *".to_string()];
        assert_eq!(tightest_cadence_ms(&crons), Some(6 * HOUR_MS));
    }

    #[test]
    fn tightest_cadence_is_none_when_nothing_is_recognised() {
        assert_eq!(tightest_cadence_ms(&["0 9 * * 1".to_string()]), None);
        assert_eq!(tightest_cadence_ms(&[]), None);
    }
}
