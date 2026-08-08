//! [`fetch_calendar_snapshot`]: the Google Calendar adapter (issue #71).
//!
//! Given a bearer token and the host-supplied selected calendar ids, fetches
//! every page of `calendars/{id}/events` (`singleEvents=true`) across the
//! rolling −7d/+90d window for every calendar, maps each item to #70's
//! [`EventRecord`], and assembles one [`CalendarSnapshot`] in memory.
//! Complete-or-nothing: any failed page fetch or any event this module
//! cannot map aborts the whole call — the caller never receives a partial
//! snapshot to persist.

use std::fmt;

use chrono::{DateTime, Duration, SecondsFormat, Utc};

use crate::calendar::snapshot::CalendarSnapshot;

use super::map::{map_event, MapError};
use super::raw::RawEventsPage;
use super::transport::{EventsTransport, TransportError};

/// The rolling window's trailing edge: 7 days before `now`.
const WINDOW_BEFORE_DAYS: i64 = 7;
/// The rolling window's leading edge: 90 days after `now`.
const WINDOW_AFTER_DAYS: i64 = 90;

#[derive(Debug)]
pub enum AdapterError {
    Transport {
        calendar_id: String,
        source: TransportError,
    },
    InvalidResponse {
        calendar_id: String,
        message: String,
    },
    Mapping {
        calendar_id: String,
        source: MapError,
    },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Transport {
                calendar_id,
                source,
            } => write!(f, "calendar {calendar_id}: transport error: {source}"),
            AdapterError::InvalidResponse {
                calendar_id,
                message,
            } => write!(f, "calendar {calendar_id}: invalid response: {message}"),
            AdapterError::Mapping {
                calendar_id,
                source,
            } => write!(f, "calendar {calendar_id}: mapping error: {source}"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// Compute the `[time_min, time_max)` RFC 3339 window bounds around `now_ms`
/// per the rolling −7d/+90d policy.
pub(super) fn window_bounds(now_ms: i64) -> (String, String) {
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms).expect("now_ms is a valid instant");
    let time_min = now - Duration::days(WINDOW_BEFORE_DAYS);
    let time_max = now + Duration::days(WINDOW_AFTER_DAYS);
    (
        time_min.to_rfc3339_opts(SecondsFormat::Secs, true),
        time_max.to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

/// Fetch and assemble one complete [`CalendarSnapshot`] across every
/// selected calendar, or fail without returning any partial snapshot.
pub async fn fetch_calendar_snapshot(
    transport: &impl EventsTransport,
    access_token: &str,
    calendar_ids: &[String],
    now_ms: i64,
) -> Result<CalendarSnapshot, AdapterError> {
    let (time_min, time_max) = window_bounds(now_ms);
    let mut events = Vec::new();

    for calendar_id in calendar_ids {
        let mut page_token: Option<String> = None;
        let mut seen_page_tokens = std::collections::HashSet::new();

        loop {
            let body = transport
                .fetch_page(
                    calendar_id,
                    access_token,
                    &time_min,
                    &time_max,
                    page_token.as_deref(),
                )
                .await
                .map_err(|source| AdapterError::Transport {
                    calendar_id: calendar_id.clone(),
                    source,
                })?;

            let page: RawEventsPage =
                serde_json::from_str(&body).map_err(|source| AdapterError::InvalidResponse {
                    calendar_id: calendar_id.clone(),
                    message: source.to_string(),
                })?;

            for raw_event in &page.items {
                let record =
                    map_event(raw_event, calendar_id).map_err(|source| AdapterError::Mapping {
                        calendar_id: calendar_id.clone(),
                        source,
                    })?;
                events.push(record);
            }

            match page.next_page_token {
                Some(token) => {
                    // Guard against a malformed or misbehaving server
                    // handing back a page token we've already requested,
                    // which would otherwise loop forever re-fetching the
                    // same page.
                    if !seen_page_tokens.insert(token.clone()) {
                        return Err(AdapterError::InvalidResponse {
                            calendar_id: calendar_id.clone(),
                            message: format!(
                                "Google returned repeated pagination token {token:?}"
                            ),
                        });
                    }
                    page_token = Some(token);
                }
                None => break,
            }
        }
    }

    Ok(CalendarSnapshot::new(events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// One scripted `(expected_page_token, response)` pair.
    type ScriptedPage = (Option<String>, Result<String, TransportError>);

    /// Ordered scripted pages per calendar. The expected token is asserted
    /// against what the adapter actually passes, so a fake that ignores
    /// pagination and re-requests page 1 forever fails the test instead of
    /// silently passing.
    struct ScriptedTransport {
        pages: Mutex<HashMap<String, Vec<ScriptedPage>>>,
    }

    impl ScriptedTransport {
        fn new(pages: HashMap<String, Vec<ScriptedPage>>) -> Self {
            Self {
                pages: Mutex::new(pages),
            }
        }
    }

    #[async_trait::async_trait]
    impl EventsTransport for ScriptedTransport {
        async fn fetch_page(
            &self,
            calendar_id: &str,
            _access_token: &str,
            _time_min: &str,
            _time_max: &str,
            page_token: Option<&str>,
        ) -> Result<String, TransportError> {
            let mut pages = self.pages.lock().unwrap();
            let queue = pages
                .get_mut(calendar_id)
                .unwrap_or_else(|| panic!("no scripted pages for calendar {calendar_id}"));
            if queue.is_empty() {
                panic!("scripted transport ran out of pages for calendar {calendar_id}");
            }
            let (expected_token, response) = queue.remove(0);
            assert_eq!(
                expected_token.as_deref(),
                page_token,
                "calendar {calendar_id}: adapter requested page_token {page_token:?}, expected {expected_token:?}"
            );
            response
        }
    }

    fn page_json(items: &str, next_page_token: Option<&str>) -> String {
        match next_page_token {
            Some(token) => format!(r#"{{"items":[{items}],"nextPageToken":"{token}"}}"#),
            None => format!(r#"{{"items":[{items}]}}"#),
        }
    }

    fn confirmed_event(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","status":"confirmed","summary":"Standup",
            "start":{{"dateTime":"2024-01-08T09:00:00-08:00","timeZone":"America/Los_Angeles"}},
            "end":{{"dateTime":"2024-01-08T09:30:00-08:00","timeZone":"America/Los_Angeles"}},
            "updated":"2024-01-01T00:00:00.000Z"}}"#
        )
    }

    #[test]
    fn window_bounds_spans_seven_days_before_and_ninety_days_after_now() {
        let now_ms = Utc
            .with_ymd_and_hms(2024, 6, 15, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let (time_min, time_max) = window_bounds(now_ms);
        assert_eq!(time_min, "2024-06-08T12:00:00Z");
        assert_eq!(time_max, "2024-09-13T12:00:00Z");
    }

    #[tokio::test]
    async fn multi_page_pagination_assembles_every_page_into_one_snapshot() {
        let mut pages = HashMap::new();
        pages.insert(
            "cal-primary".to_string(),
            vec![
                (
                    None,
                    Ok(page_json(&confirmed_event("evt-1"), Some("page-2"))),
                ),
                (
                    Some("page-2".to_string()),
                    Ok(page_json(&confirmed_event("evt-2"), None)),
                ),
            ],
        );
        let transport = ScriptedTransport::new(pages);

        let snapshot = fetch_calendar_snapshot(
            &transport,
            "token",
            &["cal-primary".to_string()],
            1_700_000_000_000,
        )
        .await
        .unwrap();

        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].provider_event_id, "evt-1");
        assert_eq!(snapshot.events[1].provider_event_id, "evt-2");
    }

    #[tokio::test]
    async fn mid_pagination_failure_yields_no_snapshot() {
        let mut pages = HashMap::new();
        pages.insert(
            "cal-primary".to_string(),
            vec![
                (
                    None,
                    Ok(page_json(&confirmed_event("evt-1"), Some("page-2"))),
                ),
                (
                    Some("page-2".to_string()),
                    Err(TransportError::new("502 from Google")),
                ),
            ],
        );
        let transport = ScriptedTransport::new(pages);

        let result = fetch_calendar_snapshot(
            &transport,
            "token",
            &["cal-primary".to_string()],
            1_700_000_000_000,
        )
        .await;

        assert!(matches!(result, Err(AdapterError::Transport { .. })));
    }

    #[tokio::test]
    async fn a_failure_on_the_second_calendar_still_yields_no_snapshot_at_all() {
        let mut pages = HashMap::new();
        pages.insert(
            "cal-a".to_string(),
            vec![(None, Ok(page_json(&confirmed_event("evt-1"), None)))],
        );
        pages.insert(
            "cal-b".to_string(),
            vec![(None, Err(TransportError::new("network error")))],
        );
        let transport = ScriptedTransport::new(pages);

        let result = fetch_calendar_snapshot(
            &transport,
            "token",
            &["cal-a".to_string(), "cal-b".to_string()],
            1_700_000_000_000,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn a_repeated_page_token_is_an_error_not_an_infinite_loop() {
        // If a server (or a misbehaving fake) handed back the same
        // nextPageToken twice, an adapter with no repeated-token guard would
        // loop forever re-requesting it. The transport only scripts two
        // responses, so an adapter that re-requests page 1 for the "page-2"
        // token would panic on running out of scripted pages instead of
        // surfacing a clean error — this pins the guard down explicitly.
        let mut pages = HashMap::new();
        pages.insert(
            "cal-primary".to_string(),
            vec![
                (
                    None,
                    Ok(page_json(&confirmed_event("evt-1"), Some("page-2"))),
                ),
                (
                    Some("page-2".to_string()),
                    Ok(page_json(&confirmed_event("evt-2"), Some("page-2"))),
                ),
            ],
        );
        let transport = ScriptedTransport::new(pages);

        let result = fetch_calendar_snapshot(
            &transport,
            "token",
            &["cal-primary".to_string()],
            1_700_000_000_000,
        )
        .await;

        assert!(matches!(result, Err(AdapterError::InvalidResponse { .. })));
    }
}
