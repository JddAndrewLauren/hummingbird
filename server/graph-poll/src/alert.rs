//! Turning a matched event into the `POST /api/alerts` body — shared
//! between the mail and calendar lanes, `gmail_poll::alert`/
//! `calendar_poll::alert`'s own pattern.

use hummingbird_domain::AlertIngest;

use crate::evaluate::Match;

/// Builds the ingest payload for one match. `raised_at` is deliberately
/// left `None` for both lanes: `m365-mail/v1` and `m365-calendar/v1` are
/// both event sources (ADR-0014's `Shape::Event`, `hummingbird_domain::
/// sources::REGISTRY`), so every instance is its own occurrence and the
/// server's own first-raise default (`now_ms`) is exactly right.
///
/// `expires_at` follows [`Match::ends_at_ms`] directly: `None` for mail
/// (`m365-mail/v1` is registered `Expiry::Never`) and `Some` for calendar
/// (`m365-calendar/v1` is registered `Expiry::Always("the instance's end
/// time")`) — one function serves both because the registry's own
/// per-source `Expiry` is exactly what decides whether a lane's
/// `Candidate`/`Match` ever carries a real `ends_at_ms` in the first place
/// (`mail_event.rs` always passes `None`; `calendar_event.rs` always passes
/// `Some`), so this function never has to consult the registry itself —
/// it just relays what the lane already decided.
pub fn plan(m: &Match) -> AlertIngest {
    AlertIngest {
        source: m.event.source.clone(),
        source_key: m.event.source_key.clone(),
        subject_key: None,
        title: m.event.title.clone(),
        body: m.event.body.clone(),
        url: m.event.url.clone(),
        severity: m.severity.clone(),
        raised_at: None,
        resolved_at: None,
        expires_at: m.ends_at_ms,
        restamp_on_change: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::Event;
    use std::collections::BTreeMap;

    fn sample_match(source: &str, severity: Option<&str>, ends_at_ms: Option<i64>) -> Match {
        Match {
            event: Event {
                source: source.into(),
                source_key: "key-1".into(),
                occurred_at: "2026-08-15T09:00".into(),
                title: "Q3 numbers".into(),
                body: Some("preview".into()),
                url: Some("https://outlook.office.com/mail/inbox/id/abc".into()),
                severity: None,
                calendar_busy: None,
                event_kind: Some("email".into()),
                extras: BTreeMap::new(),
            },
            ends_at_ms,
            severity: severity.map(str::to_string),
        }
    }

    #[test]
    fn plan_carries_the_events_identity_and_content_through() {
        let ingest = plan(&sample_match("m365-mail/v1", Some("high"), None));
        assert_eq!(ingest.source, "m365-mail/v1");
        assert_eq!(ingest.source_key, "key-1");
        assert_eq!(ingest.title, "Q3 numbers");
        assert_eq!(ingest.body.as_deref(), Some("preview"));
        assert_eq!(ingest.severity.as_deref(), Some("high"));
        assert_eq!(ingest.raised_at, None, "the server's own first-raise default applies");
        assert!(!ingest.restamp_on_change);
    }

    #[test]
    fn mail_never_carries_an_expiry() {
        let ingest = plan(&sample_match("m365-mail/v1", Some("high"), None));
        assert_eq!(ingest.expires_at, None);
    }

    #[test]
    fn calendar_carries_the_instances_end_time() {
        let ingest = plan(&sample_match("m365-calendar/v1", Some("high"), Some(1_786_875_000_000)));
        assert_eq!(ingest.expires_at, Some(1_786_875_000_000));
    }
}
