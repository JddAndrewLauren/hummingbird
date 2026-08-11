//! [`fetch_calendar_snapshot`]: the Google Calendar adapter (issue #71).
//!
//! Given a bearer token and the host-supplied [`CalendarSelection`]s, fetches
//! every page of `calendars/{id}/events` (`singleEvents=true`) across each
//! calendar's own rolling window — −7d for all, +90d or (#121's
//! [`CalendarHorizon::Long`]) +730d ahead — maps each item to #70's
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

/// The rolling window's trailing edge: 7 days before `now`. **The same for
/// every horizon** (#121): nothing in this app wants more calendar history,
/// and widening it would silently change what #122's weekend pane sees.
const WINDOW_BEFORE_DAYS: i64 = 7;
/// The rolling window's leading edge for an ordinary calendar: 90 days after
/// `now` (ADR-0005's original single window).
const WINDOW_AFTER_DAYS: i64 = 90;
/// The leading edge for a [`CalendarHorizon::Long`] calendar: two years
/// (#121). The vacation countdown's flagship case — a trip 395 days out — is
/// outside the 90-day window entirely, so a calendar answering "how long to
/// the next vacation" has to be polled further ahead than one answering
/// "what is on today".
const WINDOW_AFTER_DAYS_LONG: i64 = 730;

/// How far ahead one calendar is polled — a **policy about poll cost and
/// mirror size**, so the numbers live here in the core and the host only ever
/// says *which* calendar is which (#121, ADR-0005's amendment). A raw
/// `horizon_days` crossing the seam would give this constant a second home in
/// TypeScript, which is exactly the drift ADR-0005 puts window policy in the
/// core to avoid.
///
/// Rejected: widening [`WINDOW_AFTER_DAYS`] globally — the snapshot is a full
/// atomic replace, so the primary calendar would re-fetch two years every 15
/// minutes; and a per-calendar *role* (`primary | trips`), which smuggles a
/// standing question's vocabulary into a lane that knows nothing about
/// questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarHorizon {
    #[default]
    Standard,
    Long,
}

impl CalendarHorizon {
    fn after_days(self) -> i64 {
        match self {
            CalendarHorizon::Standard => WINDOW_AFTER_DAYS,
            CalendarHorizon::Long => WINDOW_AFTER_DAYS_LONG,
        }
    }
}

/// One calendar the host has selected, and how far ahead to poll it (#121).
/// Replaces the bare id list every layer from the picker down used to carry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CalendarSelection {
    pub id: String,
    #[serde(default)]
    pub horizon: CalendarHorizon,
}

impl CalendarSelection {
    pub fn standard(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            horizon: CalendarHorizon::Standard,
        }
    }

    pub fn long(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            horizon: CalendarHorizon::Long,
        }
    }
}

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

impl AdapterError {
    /// Whether this failure was Google rejecting the credential (HTTP 401)
    /// rather than anything retryable. #72's `ContextPoller` branches on
    /// this to hold polling and ask the host for a fresh token; a mapping or
    /// parse failure is never unauthorized, however it was provoked.
    pub fn is_unauthorized(&self) -> bool {
        match self {
            AdapterError::Transport { source, .. } => source.is_unauthorized(),
            AdapterError::InvalidResponse { .. } | AdapterError::Mapping { .. } => false,
        }
    }
}

impl std::error::Error for AdapterError {}

/// Compute the `[time_min, time_max)` RFC 3339 window bounds around `now_ms`
/// for one calendar's horizon: −7d for both, +90d or +730d ahead.
///
/// Computed **per calendar** rather than once per call (#121): the whole
/// point of a per-calendar horizon is that the trips calendar reaches two
/// years ahead while every other calendar keeps the cheap 90-day window.
pub(super) fn window_bounds(now_ms: i64, horizon: CalendarHorizon) -> (String, String) {
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms).expect("now_ms is a valid instant");
    let time_min = now - Duration::days(WINDOW_BEFORE_DAYS);
    let time_max = now + Duration::days(horizon.after_days());
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
    selections: &[CalendarSelection],
    now_ms: i64,
) -> Result<CalendarSnapshot, AdapterError> {
    let mut events = Vec::new();

    for selection in selections {
        let calendar_id = &selection.id;
        let (time_min, time_max) = window_bounds(now_ms, selection.horizon);
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
                // The page's `timeZone` is the calendar's, and it is what
                // anchors this page's all-day boundaries (see `map_event`).
                // `Ok(None)` is a deleted standalone event — a tombstone
                // with no instant to place it at, and the one raw shape the
                // mapper skips rather than fails the whole snapshot over.
                let mapped = map_event(raw_event, calendar_id, page.time_zone.as_deref()).map_err(
                    |source| AdapterError::Mapping {
                        calendar_id: calendar_id.clone(),
                        source,
                    },
                )?;
                if let Some(record) = mapped {
                    events.push(record);
                }
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
        /// `(calendar_id, time_min, time_max)` per request, so a test can
        /// assert the window each calendar was actually asked for — the only
        /// way to see that the horizon is resolved per calendar rather than
        /// once per call.
        seen_windows: Mutex<Vec<(String, String, String)>>,
    }

    impl ScriptedTransport {
        fn new(pages: HashMap<String, Vec<ScriptedPage>>) -> Self {
            Self {
                pages: Mutex::new(pages),
                seen_windows: Mutex::new(Vec::new()),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl EventsTransport for ScriptedTransport {
        async fn fetch_page(
            &self,
            calendar_id: &str,
            _access_token: &str,
            time_min: &str,
            time_max: &str,
            page_token: Option<&str>,
        ) -> Result<String, TransportError> {
            self.seen_windows.lock().unwrap().push((
                calendar_id.to_string(),
                time_min.to_string(),
                time_max.to_string(),
            ));
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

    fn at(year: i32, month: u32, day: u32) -> i64 {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn window_bounds_spans_seven_days_before_and_ninety_days_after_now() {
        let (time_min, time_max) = window_bounds(at(2024, 6, 15), CalendarHorizon::Standard);
        assert_eq!(time_min, "2024-06-08T12:00:00Z");
        assert_eq!(time_max, "2024-09-13T12:00:00Z");
    }

    #[test]
    fn a_long_horizon_reaches_two_years_ahead_and_still_only_seven_days_back() {
        // #121: the trailing edge is deliberately unchanged — nothing wants
        // more history, and widening it would change what the weekend pane
        // sees.
        let (time_min, time_max) = window_bounds(at(2024, 6, 15), CalendarHorizon::Long);
        assert_eq!(time_min, "2024-06-08T12:00:00Z");
        assert_eq!(time_max, "2026-06-15T12:00:00Z");
    }

    #[tokio::test]
    async fn each_calendar_is_queried_for_its_own_horizon_not_one_shared_window() {
        let mut pages = HashMap::new();
        pages.insert("cal-primary".to_string(), vec![(None, Ok(page_json("", None)))]);
        pages.insert("cal-trips".to_string(), vec![(None, Ok(page_json("", None)))]);
        let transport = ScriptedTransport::new(pages);

        fetch_calendar_snapshot(
            &transport,
            "token",
            &[
                CalendarSelection::standard("cal-primary"),
                CalendarSelection::long("cal-trips"),
            ],
            at(2024, 6, 15),
        )
        .await
        .unwrap();

        let seen = transport.seen_windows.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                (
                    "cal-primary".to_string(),
                    "2024-06-08T12:00:00Z".to_string(),
                    "2024-09-13T12:00:00Z".to_string()
                ),
                (
                    "cal-trips".to_string(),
                    "2024-06-08T12:00:00Z".to_string(),
                    "2026-06-15T12:00:00Z".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_selection_deserializes_from_the_hosts_json_and_defaults_to_the_standard_horizon() {
        // The wasm seam carries these as JSON text (`ffi-web`'s
        // `setCalendarSelections`), so the wire spelling is part of the
        // contract: snake_case horizons, and an absent one is standard.
        let parsed: Vec<CalendarSelection> =
            serde_json::from_str(r#"[{"id":"a","horizon":"long"},{"id":"b"}]"#).unwrap();
        assert_eq!(
            parsed,
            vec![CalendarSelection::long("a"), CalendarSelection::standard("b")]
        );
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
            &[CalendarSelection::standard("cal-primary")],
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
            &[CalendarSelection::standard("cal-primary")],
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
            &[
                CalendarSelection::standard("cal-a"),
                CalendarSelection::standard("cal-b"),
            ],
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
            &[CalendarSelection::standard("cal-primary")],
            1_700_000_000_000,
        )
        .await;

        assert!(matches!(result, Err(AdapterError::InvalidResponse { .. })));
    }
}
