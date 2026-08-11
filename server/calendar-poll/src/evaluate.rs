//! ADR-0011's evaluate-in-poll step: every fetched event, judged in memory
//! against the live rule set, before anything is persisted. Only a match
//! survives past this module — a non-matching event is dropped here and
//! never reaches `main.rs`'s alert POST, which is what makes "non-matches
//! never touch storage" true rather than aspirational. Mirrors
//! `gmail_poll::evaluate` exactly, with one addition: `google-calendar/v1`
//! is registered with `Expiry::Always("the instance's end time")`
//! (`hummingbird_domain::sources`), so a [`Match`] must carry `ends_at_ms`
//! through to `alert::plan` — `email`/`gmail_poll::Match` never needed this,
//! since `gmail/v1` never expires.

use hummingbird_domain::{higher_severity, Event, Rule};
use hummingbird_rules_engine::{evaluate_rules, RuleOutcome};

/// One fetched event, ready for evaluation, carrying the one fact
/// [`crate::event::calendar_event_to_event`] does not put on the `Event`
/// itself: the instance's end time, needed only if this event turns out to
/// match (for `expires_at`).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub event: Event,
    pub ends_at_ms: i64,
}

/// One event that matched at least one enabled rule, at the highest
/// severity among every rule that matched it — the same "N matching rules,
/// one mint at the highest severity" fold `authority::sweep::tick` and
/// `gmail_poll::evaluate` already use.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub event: Event,
    pub ends_at_ms: i64,
    pub severity: Option<String>,
}

/// Evaluates every candidate against every rule, returning only the
/// matches. An `Invalid` rule outcome (a save-time-uncaught problem —
/// practically unreachable, since `POST`/`PATCH /api/rules` already reject
/// at save) is treated as non-matching for that rule rather than aborting
/// the batch: one bad rule must not silence every other rule's evaluation
/// of every other event.
pub fn evaluate_events(rules: &[Rule], candidates: &[Candidate], now: &str) -> Vec<Match> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let severities: Vec<String> = evaluate_rules(rules, &candidate.event, now)
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
            Some(Match { event: candidate.event.clone(), ends_at_ms: candidate.ends_at_ms, severity })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::{Condition, Tier};
    use std::collections::BTreeMap;

    fn candidate(organizer: &str, ends_at_ms: i64) -> Candidate {
        let mut extras = BTreeMap::new();
        extras.insert("organizer".to_string(), hummingbird_domain::FieldValue::Str(organizer.to_string()));
        Candidate {
            event: Event {
                source: "google-calendar/v1".into(),
                source_key: "evt-1:2026-08-15T09:00:00-07:00".into(),
                occurred_at: "2026-08-15T09:00".into(),
                title: "t".into(),
                body: None,
                url: None,
                severity: None,
                calendar_busy: None,
                event_kind: Some("calendar_event".into()),
                extras,
            },
            ends_at_ms,
        }
    }

    fn rule(id: &str, organizer_eq: &str, severity: &str) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            event_kind: Some("calendar_event".into()),
            conditions: vec![Condition {
                field: "organizer".into(),
                op: "eq".into(),
                value: serde_json::json!(organizer_eq),
                negate: false,
            }],
            severity: severity.into(),
            tier: Tier::Normal,
            enabled: true,
            updated_at: 0,
            version: 1,
        }
    }

    #[test]
    fn a_matching_event_carries_its_ends_at_ms_through() {
        let matches = evaluate_events(
            &[rule("r-1", "boss@x.com", "high")],
            &[candidate("boss@x.com", 1_786_875_000_000)],
            "2026-08-15T09:00",
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].severity.as_deref(), Some("high"));
        assert_eq!(matches[0].ends_at_ms, 1_786_875_000_000);
    }

    #[test]
    fn a_non_matching_event_never_appears() {
        let matches = evaluate_events(
            &[rule("r-1", "boss@x.com", "high")],
            &[candidate("nobody@x.com", 0)],
            "2026-08-15T09:00",
        );
        assert!(matches.is_empty(), "non-matches never touch storage");
    }

    #[test]
    fn two_matching_rules_fold_to_the_higher_severity_order_independent() {
        let rules_low_first =
            vec![rule("r-low", "boss@x.com", "normal"), rule("r-high", "boss@x.com", "urgent")];
        let rules_high_first =
            vec![rule("r-high", "boss@x.com", "urgent"), rule("r-low", "boss@x.com", "normal")];
        let m1 = evaluate_events(&rules_low_first, &[candidate("boss@x.com", 0)], "2026-08-15T09:00");
        let m2 = evaluate_events(&rules_high_first, &[candidate("boss@x.com", 0)], "2026-08-15T09:00");
        assert_eq!(m1[0].severity.as_deref(), Some("urgent"));
        assert_eq!(m2[0].severity.as_deref(), Some("urgent"), "order-independent");
    }

    #[test]
    fn a_disabled_rule_never_matches() {
        let mut r = rule("r-1", "boss@x.com", "high");
        r.enabled = false;
        let matches = evaluate_events(&[r], &[candidate("boss@x.com", 0)], "2026-08-15T09:00");
        assert!(matches.is_empty());
    }
}
