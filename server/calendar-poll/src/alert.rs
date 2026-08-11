//! Turning a matched event into the `POST /api/alerts` body.

use hummingbird_domain::AlertIngest;

use crate::evaluate::Match;

/// Builds the ingest payload for one match. `raised_at` is deliberately
/// left `None`, `gmail_poll::alert::plan`'s own reason: `google-calendar/v1`
/// is an event source (ADR-0014's `Shape::Event`), so every instance is its
/// own occurrence and the server's own first-raise default (`now_ms`) is
/// exactly right.
///
/// `expires_at` is **not** left `None` the way `gmail_poll` leaves it —
/// `google-calendar/v1` is registered with
/// `Expiry::Always("the instance's end time")`
/// (`hummingbird_domain::sources::REGISTRY`), so every alert this poller
/// mints must carry it, computed from [`Match::ends_at_ms`] rather than left
/// for the authority to guess (it does not; `expires_at` is set from the
/// ingest payload verbatim, source-owned like every other field).
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
        expires_at: Some(m.ends_at_ms),
        restamp_on_change: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::Event;
    use std::collections::BTreeMap;

    fn sample_match(severity: Option<&str>) -> Match {
        Match {
            event: Event {
                source: "google-calendar/v1".into(),
                source_key: "evt-1:2026-08-15T09:00:00-07:00".into(),
                occurred_at: "2026-08-15T09:00".into(),
                title: "Board review".into(),
                body: Some("quarterly numbers".into()),
                url: Some("https://calendar.google.com/event?eid=abc".into()),
                severity: None,
                calendar_busy: None,
                event_kind: Some("calendar_event".into()),
                extras: BTreeMap::new(),
            },
            ends_at_ms: 1_786_875_000_000,
            severity: severity.map(str::to_string),
        }
    }

    #[test]
    fn plan_carries_the_events_identity_and_content_through() {
        let ingest = plan(&sample_match(Some("high")));
        assert_eq!(ingest.source, "google-calendar/v1");
        assert_eq!(ingest.source_key, "evt-1:2026-08-15T09:00:00-07:00");
        assert_eq!(ingest.title, "Board review");
        assert_eq!(ingest.body.as_deref(), Some("quarterly numbers"));
        assert_eq!(ingest.severity.as_deref(), Some("high"));
        assert_eq!(ingest.raised_at, None, "the server's own first-raise default applies");
        assert!(!ingest.restamp_on_change);
    }

    #[test]
    fn expires_at_is_the_instances_end_time() {
        let ingest = plan(&sample_match(Some("high")));
        assert_eq!(ingest.expires_at, Some(1_786_875_000_000));
    }
}
