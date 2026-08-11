//! Turning a matched event into the `POST /api/alerts` body.

use hummingbird_domain::AlertIngest;

use crate::evaluate::Match;

/// Builds the ingest payload for one match. `raised_at` is deliberately
/// left `None`: `gmail/v1` is an event source (ADR-0014's `Shape::Event`),
/// so every message is its own occurrence and the server's own first-raise
/// default (`now_ms`) is exactly right; there is no "still live" state to
/// preserve the way `item-threshold/v1`'s state-source re-raises do. A
/// second POST for the same message id (a re-evaluated batch after a lost
/// cursor, ADR-0011's own "re-fetch-and-upsert, which the dedupe key
/// absorbs") carries byte-identical `title`/`body`/`url`/`severity`, so the
/// upsert's own `next == current` no-op (`alerts::upsert`) is what actually
/// keeps this idempotent — never a check made here.
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
        expires_at: None,
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
                source: "gmail/v1".into(),
                source_key: "m-1".into(),
                occurred_at: "2026-08-15T09:00".into(),
                title: "Q3 numbers".into(),
                body: Some("preview".into()),
                url: Some("https://mail.google.com/mail/u/0/#all/m-1".into()),
                severity: None,
                calendar_busy: None,
                event_kind: Some("email".into()),
                extras: BTreeMap::new(),
            },
            severity: severity.map(str::to_string),
        }
    }

    #[test]
    fn plan_carries_the_events_identity_and_content_through() {
        let ingest = plan(&sample_match(Some("high")));
        assert_eq!(ingest.source, "gmail/v1");
        assert_eq!(ingest.source_key, "m-1");
        assert_eq!(ingest.title, "Q3 numbers");
        assert_eq!(ingest.body.as_deref(), Some("preview"));
        assert_eq!(ingest.severity.as_deref(), Some("high"));
        assert_eq!(ingest.raised_at, None, "the server's own first-raise default applies");
        assert!(!ingest.restamp_on_change);
    }
}
