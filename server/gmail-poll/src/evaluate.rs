//! ADR-0011's evaluate-in-poll step: every fetched event, judged in memory
//! against the live rule set, before anything is persisted. Only a match
//! survives past this module — a non-matching event is dropped here and
//! never reaches `main.rs`'s alert POST, which is what makes "non-matches
//! never touch storage" true rather than aspirational.

use hummingbird_domain::{higher_severity, Event, Rule};
use hummingbird_rules_engine::{evaluate_rules, RuleOutcome};

/// One event that matched at least one enabled rule, at the highest
/// severity among every rule that matched it — the same "N matching rules,
/// one mint at the highest severity" fold `authority::sweep::tick` already
/// uses for `item_threshold`, applied here across an in-memory batch
/// instead of a DO alarm tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub event: Event,
    pub severity: Option<String>,
}

/// Evaluates every event against every rule, returning only the matches.
/// An `Invalid` rule outcome (a save-time-uncaught problem — practically
/// unreachable, since `POST`/`PATCH /api/rules` already reject at save)
/// is treated as non-matching for that rule rather than aborting the
/// batch: one bad rule must not silence every other rule's evaluation of
/// every other event.
pub fn evaluate_events(rules: &[Rule], events: &[Event], now: &str) -> Vec<Match> {
    events
        .iter()
        .filter_map(|event| {
            let severities: Vec<String> = evaluate_rules(rules, event, now)
                .into_iter()
                .filter_map(|(_, outcome)| match outcome {
                    RuleOutcome::Matched(verdict) => Some(verdict.severity),
                    _ => None,
                })
                .collect();
            if severities.is_empty() {
                return None;
            }
            let severity = severities
                .iter()
                .fold(None::<&str>, |acc, incoming| higher_severity(acc, Some(incoming.as_str())))
                .map(str::to_string);
            Some(Match { event: event.clone(), severity })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::{Condition, Tier};
    use std::collections::BTreeMap;

    fn event(from: &str) -> Event {
        let mut extras = BTreeMap::new();
        extras.insert(
            "from".to_string(),
            hummingbird_domain::FieldValue::Str(from.to_string()),
        );
        Event {
            source: "gmail/v1".into(),
            source_key: "m-1".into(),
            occurred_at: "2026-08-15T09:00".into(),
            title: "t".into(),
            body: None,
            url: None,
            severity: None,
            calendar_busy: None,
            event_kind: Some("email".into()),
            extras,
        }
    }

    fn rule(id: &str, from_eq: &str, severity: &str) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            event_kind: Some("email".into()),
            conditions: vec![Condition {
                field: "from".into(),
                op: "eq".into(),
                value: serde_json::json!(from_eq),
                negate: false,
            }],
            severity: severity.into(),
            tier: Tier::Normal,
            enabled: true,
            updated_at: 0,
            version: 1,
            deleted_at: None,
        }
    }

    #[test]
    fn a_matching_event_is_returned_with_the_matched_severity() {
        let matches = evaluate_events(&[rule("r-1", "boss@x.com", "high")], &[event("boss@x.com")], "2026-08-15T09:00");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].severity.as_deref(), Some("high"));
    }

    #[test]
    fn a_non_matching_event_never_appears() {
        let matches = evaluate_events(&[rule("r-1", "boss@x.com", "high")], &[event("nobody@x.com")], "2026-08-15T09:00");
        assert!(matches.is_empty(), "non-matches never touch storage");
    }

    #[test]
    fn two_matching_rules_fold_to_the_higher_severity_order_independent() {
        let rules_low_first =
            vec![rule("r-low", "boss@x.com", "normal"), rule("r-high", "boss@x.com", "urgent")];
        let rules_high_first =
            vec![rule("r-high", "boss@x.com", "urgent"), rule("r-low", "boss@x.com", "normal")];
        let m1 = evaluate_events(&rules_low_first, &[event("boss@x.com")], "2026-08-15T09:00");
        let m2 = evaluate_events(&rules_high_first, &[event("boss@x.com")], "2026-08-15T09:00");
        assert_eq!(m1[0].severity.as_deref(), Some("urgent"));
        assert_eq!(m2[0].severity.as_deref(), Some("urgent"), "order-independent");
    }

    #[test]
    fn a_disabled_rule_never_matches() {
        let mut r = rule("r-1", "boss@x.com", "high");
        r.enabled = false;
        let matches = evaluate_events(&[r], &[event("boss@x.com")], "2026-08-15T09:00");
        assert!(matches.is_empty());
    }
}
