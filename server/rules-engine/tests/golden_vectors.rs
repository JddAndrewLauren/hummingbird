//! Golden test vectors for the rule engine (#133, ADR-0013), extending the
//! repo's frozen-vector discipline (`client/core/src/sync/write/id.rs`):
//! a fixed input, a fixed expected output, and a comment on *why* that
//! output is the only correct one. These are the acceptance criteria made
//! executable, one vector at a time — a regression here means the rule
//! engine started disagreeing with the ADR, not that the vector is stale.

use std::collections::BTreeMap;

use hummingbird_domain::{Condition, Event, FieldValue, Rule, Tier};
use hummingbird_rules_engine::{evaluate_rule, validate_rule, RuleOutcome, RuleProblem, Verdict};

fn rule(id: &str, event_kind: Option<&str>, conditions: Vec<Condition>) -> Rule {
    Rule {
        id: id.to_string(),
        name: id.to_string(),
        event_kind: event_kind.map(str::to_string),
        conditions,
        severity: "high".to_string(),
        tier: Tier::Normal,
        enabled: true,
        updated_at: 0,
        version: 1,
    }
}

fn condition(field: &str, op: &str, value: serde_json::Value, negate: bool) -> Condition {
    Condition { field: field.to_string(), op: op.to_string(), value, negate }
}

fn bare_event(kind: Option<&str>) -> Event {
    Event {
        source: "gmail/v1".to_string(),
        source_key: "msg-1".to_string(),
        occurred_at: "2026-08-15T09:00".to_string(),
        title: "t".to_string(),
        body: None,
        url: None,
        severity: None,
        calendar_busy: None,
        event_kind: kind.map(str::to_string),
        extras: BTreeMap::new(),
    }
}

fn with_extras(mut event: Event, extras: Vec<(&str, FieldValue)>) -> Event {
    for (k, v) in extras {
        event.extras.insert(k.to_string(), v);
    }
    event
}

/// **Vector: overdue still matches under `within_next`.** ADR-0013's
/// `item_threshold` example: a deadline in the past must still satisfy
/// `within_next`, since it is unbounded on the past side by design — the
/// rule that would otherwise quit precisely when the item is most urgent.
#[test]
fn overdue_deadline_still_matches_within_next() {
    let r = rule(
        "overdue",
        Some("item_threshold"),
        vec![condition("deadline", "within_next", serde_json::json!("2h"), false)],
    );
    let event = with_extras(
        bare_event(Some("item_threshold")),
        vec![("deadline", FieldValue::Str("2020-01-01T00:00".to_string()))],
    );
    let outcome = evaluate_rule(&r, &event, "2026-08-15T09:00");
    assert_eq!(
        outcome,
        RuleOutcome::Matched(Verdict {
            rule_id: "overdue".to_string(),
            severity: "high".to_string(),
            tier: Tier::Normal
        })
    );
}

/// A deadline safely in the future does *not* match a short `within_next`
/// window — the boundary case that proves the vector above isn't just
/// "always matches."
#[test]
fn a_far_future_deadline_does_not_match_a_short_within_next() {
    let r = rule(
        "soon",
        Some("item_threshold"),
        vec![condition("deadline", "within_next", serde_json::json!("2h"), false)],
    );
    let event = with_extras(
        bare_event(Some("item_threshold")),
        vec![("deadline", FieldValue::Str("2030-01-01T00:00".to_string()))],
    );
    assert_eq!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::NotMatched);
}

/// **Vector: a missing field under negation evaluates false.** Negation
/// must not resurrect an absent field — `labels contains 'x'` negated on
/// an event with no `labels` extra at all stays false, not true.
#[test]
fn missing_field_under_negation_is_false() {
    let r = rule(
        "no-alert-label",
        Some("email"),
        vec![condition("labels", "contains", serde_json::json!("alert-high"), true)],
    );
    let event = bare_event(Some("email")); // no `labels` extra populated
    assert_eq!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::NotMatched);
}

/// A present, non-matching field under negation *does* match — the
/// contrast case proving the vector above is about absence specifically,
/// not negation being broken generally.
#[test]
fn a_present_non_matching_field_under_negation_matches() {
    let r = rule(
        "no-alert-label",
        Some("email"),
        vec![condition("labels", "contains", serde_json::json!("alert-high"), true)],
    );
    let event = with_extras(
        bare_event(Some("email")),
        vec![("labels", FieldValue::StrList(vec!["inbox".to_string()]))],
    );
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// **Vector: a stale or missing busy snapshot defaults to free.**
/// `calendar_busy` is the sole exception to "missing means false" — a
/// missing snapshot resolves to *not busy*, over-suppression being the
/// failure ADR-0013 avoids.
#[test]
fn missing_calendar_busy_snapshot_defaults_to_not_busy() {
    let free_rule = rule("quiet", None, vec![condition("calendar_busy", "is", serde_json::json!(false), false)]);
    let busy_rule = rule("busy", None, vec![condition("calendar_busy", "is", serde_json::json!(true), false)]);
    let mut event = bare_event(None);
    event.calendar_busy = None; // missing/stale snapshot

    assert!(matches!(evaluate_rule(&free_rule, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
    assert_eq!(evaluate_rule(&busy_rule, &event, "2026-08-15T09:00"), RuleOutcome::NotMatched);
}

/// **Vector: a NULL `event_kind` rule evaluates against Event core for
/// events of every kind.** The same rule, referencing only a core field,
/// matches an `email` event and an `item_threshold` event alike.
#[test]
fn null_event_kind_rule_matches_core_fields_on_events_of_any_kind() {
    let r = rule("any-kind", None, vec![condition("source", "eq", serde_json::json!("gmail/v1"), false)]);

    let email_event = bare_event(Some("email"));
    let item_event = bare_event(Some("item_threshold"));

    assert!(matches!(evaluate_rule(&r, &email_event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
    assert!(matches!(evaluate_rule(&r, &item_event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// **Vector: an operator illegal for a field's type is rejected, not
/// silently coerced.** `priority` is `number`-typed (item_threshold);
/// `contains` is not in the number operator set.
#[test]
fn illegal_operator_for_field_type_is_invalid_not_coerced() {
    let r = rule(
        "bad-op",
        Some("item_threshold"),
        vec![condition("priority", "contains", serde_json::json!("2"), false)],
    );
    let event = with_extras(
        bare_event(Some("item_threshold")),
        vec![("priority", FieldValue::Num(2.0))],
    );
    let outcome = evaluate_rule(&r, &event, "2026-08-15T09:00");
    assert_eq!(
        outcome,
        RuleOutcome::Invalid(vec![RuleProblem::IllegalOperator {
            field: "priority".to_string(),
            op: "contains".to_string(),
        }])
    );
}

/// **Vector: a list `value` matches any-of; a string operator over a list
/// field matches on any element; matching is case-insensitive.** The
/// event's `labels` list has one element containing the (differently
/// cased) substring from one element of the condition's list value.
#[test]
fn list_value_any_of_over_list_field_is_case_insensitive() {
    let r = rule(
        "alert-label",
        Some("email"),
        vec![condition(
            "labels",
            "contains",
            serde_json::json!(["ALERT-HIGH", "urgent-flag"]),
            false,
        )],
    );
    let event = with_extras(
        bare_event(Some("email")),
        vec![(
            "labels",
            FieldValue::StrList(vec!["inbox".to_string(), "alert-high-priority".to_string()]),
        )],
    );
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// **Vector: the registry's JSON export is produced by the same
/// definition the engine evaluates against.** `priority`'s declared type
/// in the JSON export is exactly the type the engine gates `contains`
/// against above — one definition, two consumers, checked here by reading
/// the export directly rather than a hand-copied constant.
#[test]
fn registry_json_export_agrees_with_the_types_the_engine_gates_on() {
    let json = hummingbird_domain::kind_registry_json();
    let item_threshold = json.as_array().unwrap().iter().find(|k| k["key"] == "item_threshold").unwrap();
    let priority = item_threshold["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "priority")
        .unwrap();
    assert_eq!(priority["field_type"], "number");
}

/// **Vector: a rule naming a field its kind no longer declares is
/// reported invalid rather than never firing.** `email` does not declare
/// `nonexistent_field`.
#[test]
fn rule_naming_an_undeclared_field_is_invalid() {
    let r = rule(
        "typo",
        Some("email"),
        vec![condition("nonexistent_field", "eq", serde_json::json!("x"), false)],
    );
    let event = bare_event(Some("email"));
    let outcome = evaluate_rule(&r, &event, "2026-08-15T09:00");
    assert_eq!(
        outcome,
        RuleOutcome::Invalid(vec![RuleProblem::UnknownField { field: "nonexistent_field".to_string() }])
    );

    // Observable independent of any event too (#140's load-time check).
    assert_eq!(
        validate_rule(&r),
        vec![RuleProblem::UnknownField { field: "nonexistent_field".to_string() }]
    );
}

/// A rule naming an `event_kind` the registry has never heard of is
/// invalid for the same reason — a retired or mistyped kind must not look
/// like a rule that simply never matches.
#[test]
fn rule_naming_an_unknown_kind_is_invalid() {
    let r = rule("ghost-kind", Some("weather_alert"), vec![condition("temp", "gt", serde_json::json!(90), false)]);
    let event = bare_event(Some("weather_alert"));
    let outcome = evaluate_rule(&r, &event, "2026-08-15T09:00");
    match outcome {
        RuleOutcome::Invalid(problems) => {
            assert!(problems.contains(&RuleProblem::UnknownKind { event_kind: "weather_alert".to_string() }));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

/// `email` golden vector: `subject contains 'urgent'`, case-insensitive,
/// against a subject that carries the substring in different casing.
#[test]
fn email_kind_subject_contains_matches_case_insensitively() {
    let r = rule(
        "urgent-subject",
        Some("email"),
        vec![condition("subject", "contains", serde_json::json!("urgent"), false)],
    );
    let event = with_extras(
        bare_event(Some("email")),
        vec![("subject", FieldValue::Str("URGENT: renew now".to_string()))],
    );
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// `calendar_event` golden vector: `is_all_day is false` on a timed
/// meeting.
#[test]
fn calendar_event_kind_is_all_day_matches() {
    let r = rule(
        "timed-meeting",
        Some("calendar_event"),
        vec![condition("is_all_day", "is", serde_json::json!(false), false)],
    );
    let event = with_extras(
        bare_event(Some("calendar_event")),
        vec![("is_all_day", FieldValue::Bool(false))],
    );
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// `snapshot_change` golden vector: the flagship ADR-0013 scenario needs
/// no value condition at all — `key eq 'trash'` alone routes and tiers.
#[test]
fn snapshot_change_kind_key_eq_matches_with_no_value_condition() {
    let r = rule(
        "trash-slide",
        Some("snapshot_change"),
        vec![condition("key", "eq", serde_json::json!("trash"), false)],
    );
    let event = with_extras(
        bare_event(Some("snapshot_change")),
        vec![("key", FieldValue::Str("trash".to_string()))],
    );
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// `snapshot_change`'s `value`/`previous` are `Dynamic` (declared per key
/// at wiring time, ADR-0013) — `gt` is legal once the actual value on the
/// event is numeric.
#[test]
fn snapshot_change_dynamic_value_field_supports_gt_when_numeric() {
    let r = rule(
        "big-jump",
        Some("snapshot_change"),
        vec![condition("value", "gt", serde_json::json!(0.8), false)],
    );
    let event = with_extras(bare_event(Some("snapshot_change")), vec![("value", FieldValue::Num(0.95))]);
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// A `Dynamic` field rejects an operator illegal for the value it actually
/// holds — `contains` has no meaning against a numeric snapshot value.
#[test]
fn snapshot_change_dynamic_value_field_rejects_a_mismatched_operator() {
    let r = rule(
        "bad-dynamic-op",
        Some("snapshot_change"),
        vec![condition("value", "contains", serde_json::json!("x"), false)],
    );
    let event = with_extras(bare_event(Some("snapshot_change")), vec![("value", FieldValue::Num(0.95))]);
    assert_eq!(
        evaluate_rule(&r, &event, "2026-08-15T09:00"),
        RuleOutcome::Invalid(vec![RuleProblem::IllegalOperator {
            field: "value".to_string(),
            op: "contains".to_string(),
        }])
    );
}

/// `alert_raised` golden vector: `mints: false` — core fields carry it
/// entirely, and the registry still says it never mints.
#[test]
fn alert_raised_kind_matches_on_core_fields_and_does_not_mint() {
    assert!(!hummingbird_domain::find_kind("alert_raised").unwrap().mints);

    let r = rule(
        "pushed-critical",
        Some("alert_raised"),
        vec![condition("severity", "eq", serde_json::json!("critical"), false)],
    );
    let mut event = bare_event(Some("alert_raised"));
    event.severity = Some("critical".to_string());
    assert!(matches!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::Matched(_)));
}

/// A rule with a different `event_kind` than the event simply does not
/// apply — not an error, just non-applicable.
#[test]
fn a_rule_for_a_different_kind_does_not_match() {
    let r = rule("email-only", Some("email"), vec![condition("subject", "contains", serde_json::json!("x"), false)]);
    let event = bare_event(Some("calendar_event"));
    assert_eq!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::NotMatched);
}

/// A disabled rule never matches, regardless of its conditions.
#[test]
fn a_disabled_rule_never_matches() {
    let mut r = rule("off", None, vec![condition("source", "eq", serde_json::json!("gmail/v1"), false)]);
    r.enabled = false;
    let event = bare_event(Some("email"));
    assert_eq!(evaluate_rule(&r, &event, "2026-08-15T09:00"), RuleOutcome::NotMatched);
}
