//! The `deadline` date/time picker's two conversions (#140, acceptance
//! criterion 3) — **one of ADR-0025's two named M4 drifts**
//! (`client/web/src/screens/rules/deadline-picker.ts:32`).
//!
//! What that module re-derived was the civil-time arithmetic itself: it
//! held its own `YYYY-MM-DDTHH:MM` formatter, its own parse, and its own
//! ± duration in epoch milliseconds, beside
//! [`hummingbird_domain::deadline`]'s. Both directions below are now
//! [`hummingbird_domain::shift`] and [`hummingbird_domain::minutes_until`],
//! which is what makes the picker share the day-only → `T23:59` convention
//! with the sort key and the rule evaluator instead of agreeing with them
//! by coincidence.
//!
//! **The picker writes a duration, not a moment.** ADR-0013's wire value
//! for `deadline within_next` is an ordinary duration literal; picking a
//! target date is only the more natural *gesture*. So `now` is an
//! ephemeral read-time clock (the device's own wall clock at the moment of
//! the edit) and arrives as an already-resolved deadline-shaped string —
//! this crate resolves no civil date to an instant, per ADR-0015.
//!
//! **Sign convention, matching [`super::backtest`]'s own:**
//! `within_next` fires when `target <= now + D`, `within_last` when
//! `target >= now - D`. Picking a target on the "wrong" side (a past date
//! for `within_next`) still produces a well-formed duration — clamped up to
//! one minute, since [`super::duration::parse_duration_ms`] rejects a
//! non-positive amount — rather than a value the picker silently refuses to
//! write.

use hummingbird_domain::{is_valid_deadline, minutes_until, shift, DurationUnit};
use hummingbird_rules_engine::Operator;

use super::duration::format_duration;

/// The signed shift `op` applies, or `None` for an operator that is not one
/// of the relative-time pair. Never inferred from the literal: ADR-0013's
/// direction is always the operator.
fn signed(op: Operator, amount: i64) -> Option<i64> {
    match op {
        Operator::WithinNext => Some(amount),
        Operator::WithinLast => Some(-amount),
        _ => None,
    }
}

/// The concrete moment `duration_value` (the wire's `"2h"`/`"3d"` string)
/// displays as, given `op` and the clock `now`. Empty when `duration_value`
/// does not parse, when `op` is not a relative-time operator, or when `now`
/// is not deadline-shaped — the picker then starts blank rather than
/// guessing a moment nobody chose.
pub fn datetime_input_value_from_duration(
    duration_value: &str,
    op: Operator,
    now: &str,
) -> String {
    let Some((amount, unit)) = parse_positive_duration(duration_value) else {
        return String::new();
    };
    let Some(amount) = signed(op, amount) else {
        return String::new();
    };
    shift(now, amount, unit).unwrap_or_default()
}

/// The wire duration literal for a picked moment, given `op` and the clock
/// `now`. Always expressed in **whole minutes** — exact, and
/// [`super::duration::duration_units_for`] already permits a bare `m`
/// suffix with no upper bound on a `timestamp` field, which `deadline` is.
///
/// `None` when `input_value` is not (yet) a complete moment, so a mid-edit
/// keystroke never writes a bogus condition value.
pub fn duration_from_datetime_input_value(
    input_value: &str,
    op: Operator,
    now: &str,
) -> Option<String> {
    if !is_valid_deadline(input_value) {
        return None;
    }
    let ahead = minutes_until(input_value, now)?;
    let minutes = match op {
        Operator::WithinNext => ahead,
        Operator::WithinLast => -ahead,
        _ => return None,
    };
    Some(format_duration(minutes.max(1), DurationUnit::Minutes))
}

/// [`super::duration::parse_duration_ms`]'s reject-zero rule, kept in the
/// `(amount, unit)` shape [`shift`] takes — going through milliseconds and
/// back would re-introduce exactly the unit arithmetic this module exists
/// to stop doing twice.
fn parse_positive_duration(value: &str) -> Option<(i64, DurationUnit)> {
    let (amount, unit) = hummingbird_domain::parse_duration(value.trim())?;
    (amount > 0).then_some((amount, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-08-15T09:30";

    #[test]
    fn resolves_a_within_next_duration_to_a_future_moment() {
        assert_eq!(
            datetime_input_value_from_duration("2h", Operator::WithinNext, NOW),
            "2026-08-15T11:30",
        );
    }

    #[test]
    fn resolves_a_within_last_duration_to_a_past_moment() {
        assert_eq!(
            datetime_input_value_from_duration("3d", Operator::WithinLast, NOW),
            "2026-08-12T09:30",
        );
    }

    #[test]
    fn is_empty_for_an_unparseable_stored_value() {
        assert_eq!(datetime_input_value_from_duration("", Operator::WithinNext, NOW), "");
        assert_eq!(datetime_input_value_from_duration("soon", Operator::WithinNext, NOW), "");
        assert_eq!(datetime_input_value_from_duration("0m", Operator::WithinNext, NOW), "");
    }

    #[test]
    fn is_empty_for_an_operator_outside_the_relative_time_pair() {
        assert_eq!(datetime_input_value_from_duration("2h", Operator::Eq, NOW), "");
    }

    #[test]
    fn computes_a_within_next_duration_in_whole_minutes() {
        assert_eq!(
            duration_from_datetime_input_value("2026-08-15T11:30", Operator::WithinNext, NOW)
                .as_deref(),
            Some("120m"),
        );
    }

    #[test]
    fn computes_a_within_last_duration_in_whole_minutes() {
        assert_eq!(
            duration_from_datetime_input_value("2026-08-12T09:30", Operator::WithinLast, NOW)
                .as_deref(),
            Some("4320m"),
        );
    }

    #[test]
    fn clamps_a_moment_on_the_wrong_side_of_now_to_one_minute() {
        assert_eq!(
            duration_from_datetime_input_value("2026-08-10T09:30", Operator::WithinNext, NOW)
                .as_deref(),
            Some("1m"),
        );
        assert_eq!(
            duration_from_datetime_input_value("2026-08-20T09:30", Operator::WithinLast, NOW)
                .as_deref(),
            Some("1m"),
        );
    }

    #[test]
    fn is_none_for_a_mid_edit_incomplete_input_value() {
        assert_eq!(
            duration_from_datetime_input_value("", Operator::WithinNext, NOW),
            None,
        );
        assert_eq!(
            duration_from_datetime_input_value("2026-08-15T11", Operator::WithinNext, NOW),
            None,
        );
    }

    #[test]
    fn round_trips_a_duration_through_the_displayed_moment() {
        let displayed = datetime_input_value_from_duration("90m", Operator::WithinNext, NOW);
        assert_eq!(
            duration_from_datetime_input_value(&displayed, Operator::WithinNext, NOW).as_deref(),
            Some("90m"),
        );
    }
}
