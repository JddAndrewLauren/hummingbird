//! The `within_next`/`within_last` value grammar (ADR-0013): a bare
//! positive integer plus a unit suffix. Sunk from
//! `client/web/src/screens/rules/duration.ts`, which carried its own
//! `/^(\d+)(m|h|d)$/` regex and its own unit → milliseconds table beside
//! [`hummingbird_domain::parse_duration`]'s.
//!
//! This module parses, formats and measures against the Durable Object's
//! alarm interval (#138). It never decides legality — [`super::operators`]
//! does — and it never rejects a save: #133's `validate_rule` is the
//! save-time gate, server-side.

use hummingbird_domain::DurationUnit;
use hummingbird_domain::FieldType;

/// The wire suffix for one unit — [`parse_duration_unit`]'s exact inverse.
/// [`hummingbird_domain::DurationUnit`] carries no such spelling of its own
/// (the engine only ever parses), so the one copy lives here, next to the
/// editor that has to write the literal back.
pub fn duration_unit_str(unit: DurationUnit) -> &'static str {
    match unit {
        DurationUnit::Minutes => "m",
        DurationUnit::Hours => "h",
        DurationUnit::Days => "d",
    }
}

/// One unit suffix, parsed. `None` for anything but `m`/`h`/`d`.
pub fn parse_duration_unit(s: &str) -> Option<DurationUnit> {
    match s {
        "m" => Some(DurationUnit::Minutes),
        "h" => Some(DurationUnit::Hours),
        "d" => Some(DurationUnit::Days),
        _ => None,
    }
}

fn unit_ms(unit: DurationUnit) -> i64 {
    match unit {
        DurationUnit::Minutes => 60_000,
        DurationUnit::Hours => 60 * 60_000,
        DurationUnit::Days => 24 * 60 * 60_000,
    }
}

/// A wire duration literal (`"2h"`, `"10m"`, `"3d"`) in milliseconds —
/// the same value ADR-0013's engine measures a condition against.
///
/// `None` for anything [`hummingbird_domain::parse_duration`] rejects, and
/// additionally for a **zero** amount: `"0m"` parses upstream (the engine
/// has no reason to care — a zero shift is simply `now`) but is not a
/// duration a picker may offer or a warning may be measured from, and the
/// retired TS module rejected it too. Leading/trailing whitespace is
/// trimmed first, matching what a text input actually hands over.
pub fn parse_duration_ms(value: &str) -> Option<i64> {
    let (amount, unit) = hummingbird_domain::parse_duration(value.trim())?;
    if amount <= 0 {
        return None;
    }
    amount.checked_mul(unit_ms(unit))
}

/// The wire literal for `amount` of `unit` — [`parse_duration_ms`]'s
/// inverse, for a duration picker to write back.
pub fn format_duration(amount: i64, unit: DurationUnit) -> String {
    format!("{amount}{}", duration_unit_str(unit))
}

/// The units a duration picker offers for one field type — ADR-0013's own
/// table, and the same restriction `validate_rule` enforces at save time
/// (`"a \`date\`-typed field accepts \`d\` units only"`): a sub-day offset
/// against a day-only value is meaningless, so a `date` field is
/// day-grained only and everything else gets all three.
pub fn duration_units_for(field_type: FieldType) -> Vec<DurationUnit> {
    match field_type {
        FieldType::Date => vec![DurationUnit::Days],
        _ => vec![
            DurationUnit::Minutes,
            DurationUnit::Hours,
            DurationUnit::Days,
        ],
    }
}

/// **Warn — never reject** — when a duration is shorter than the DO alarm
/// interval (#138): a rule that fires less precisely than its author
/// intended is still legitimate, so this is read-only material for a
/// warning banner, never a save gate. An unparseable literal warns nothing
/// — a malformed duration is #133's save-time rejection to catch, not this
/// one's.
pub fn is_below_alarm_interval(value: &str, alarm_interval_ms: i64) -> bool {
    parse_duration_ms(value).is_some_and(|ms| ms < alarm_interval_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minutes_hours_and_days() {
        assert_eq!(parse_duration_ms("10m"), Some(10 * 60_000));
        assert_eq!(parse_duration_ms("2h"), Some(2 * 60 * 60_000));
        assert_eq!(parse_duration_ms("3d"), Some(3 * 24 * 60 * 60_000));
    }

    #[test]
    fn rejects_a_zero_or_negative_amount() {
        assert_eq!(parse_duration_ms("0m"), None);
        assert_eq!(parse_duration_ms("-3d"), None);
    }

    #[test]
    fn rejects_an_unrecognised_unit_or_shape() {
        assert_eq!(parse_duration_ms("3w"), None);
        assert_eq!(parse_duration_ms("soon"), None);
        assert_eq!(parse_duration_ms(""), None);
    }

    #[test]
    fn trims_before_parsing() {
        assert_eq!(parse_duration_ms("  2h "), Some(2 * 60 * 60_000));
    }

    #[test]
    fn format_duration_is_parse_duration_ms_s_inverse() {
        assert_eq!(format_duration(2, DurationUnit::Hours), "2h");
        assert_eq!(
            parse_duration_ms(&format_duration(2, DurationUnit::Hours)),
            parse_duration_ms("2h"),
        );
    }

    #[test]
    fn a_date_field_is_day_grained_only() {
        assert_eq!(duration_units_for(FieldType::Date), [DurationUnit::Days]);
        assert_eq!(
            duration_units_for(FieldType::Timestamp),
            [
                DurationUnit::Minutes,
                DurationUnit::Hours,
                DurationUnit::Days
            ],
        );
    }

    #[test]
    fn warns_only_strictly_below_the_alarm_interval() {
        let alarm = 15 * 60_000;
        assert!(is_below_alarm_interval("5m", alarm));
        assert!(!is_below_alarm_interval("15m", alarm));
        assert!(!is_below_alarm_interval("1h", alarm));
    }

    #[test]
    fn warns_nothing_for_an_unparseable_duration() {
        assert!(!is_below_alarm_interval("soon", 15 * 60_000));
    }

    #[test]
    fn unit_spelling_round_trips() {
        for unit in [
            DurationUnit::Minutes,
            DurationUnit::Hours,
            DurationUnit::Days,
        ] {
            assert_eq!(parse_duration_unit(duration_unit_str(unit)), Some(unit));
        }
        assert_eq!(parse_duration_unit("w"), None);
    }
}
