//! Fixture-driven acceptance tests for the Google Calendar adapter (#71).
//!
//! No live credentials or network calls anywhere (#46 acceptance): every
//! response body below is a committed fixture under
//! `tests/fixtures/google/`, served by an in-memory [`FixtureTransport`].

use std::collections::HashMap;
use std::sync::Mutex;

use hummingbird_core::calendar::google::{
    fetch_calendar_snapshot, EventsTransport, TransportError,
};
use hummingbird_core::calendar::{EventStatus, EventTime};

/// Any instant works for these tests — the adapter's window policy itself
/// is covered separately; these tests only check mapping and pagination.
const NOW_MS: i64 = 1_706_400_000_000; // 2024-01-28T00:00:00Z

fn fixture(name: &str) -> String {
    match name {
        "recurrence_expansion" => {
            include_str!("fixtures/google/recurrence_expansion.json").to_string()
        }
        "cancelled_instance" => include_str!("fixtures/google/cancelled_instance.json").to_string(),
        "all_day_boundaries" => include_str!("fixtures/google/all_day_boundaries.json").to_string(),
        "dst_transition" => include_str!("fixtures/google/dst_transition.json").to_string(),
        "pagination_page_1" => include_str!("fixtures/google/pagination_page_1.json").to_string(),
        "pagination_page_2" => include_str!("fixtures/google/pagination_page_2.json").to_string(),
        other => panic!("unknown fixture {other}"),
    }
}

/// Serves a scripted sequence of page responses per calendar id, popped in
/// call order. A `None` entry stands in for a transport failure.
struct FixtureTransport {
    pages: Mutex<HashMap<String, Vec<Option<String>>>>,
}

impl FixtureTransport {
    fn single_calendar(calendar_id: &str, pages: Vec<Option<String>>) -> Self {
        let mut map = HashMap::new();
        map.insert(calendar_id.to_string(), pages);
        Self {
            pages: Mutex::new(map),
        }
    }
}

#[async_trait::async_trait]
impl EventsTransport for FixtureTransport {
    async fn fetch_page(
        &self,
        calendar_id: &str,
        _access_token: &str,
        _time_min: &str,
        _time_max: &str,
        _page_token: Option<&str>,
    ) -> Result<String, TransportError> {
        let mut pages = self.pages.lock().unwrap();
        let queue = pages
            .get_mut(calendar_id)
            .unwrap_or_else(|| panic!("no fixture pages scripted for calendar {calendar_id}"));
        if queue.is_empty() {
            panic!("fixture transport ran out of pages for calendar {calendar_id}");
        }
        match queue.remove(0) {
            Some(body) => Ok(body),
            None => Err(TransportError::new("simulated transport failure")),
        }
    }
}

#[tokio::test]
async fn recurrence_expansion_yields_one_instance_per_series_occurrence() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![Some(fixture("recurrence_expansion"))],
    );

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 3);
    let ids: Vec<&str> = snapshot
        .events
        .iter()
        .map(|e| e.provider_event_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "series-1_20240108T170000Z",
            "series-1_20240115T170000Z",
            "series-1_20240122T170000Z",
        ]
    );

    // Each instance's identity combines the series id with its own
    // original start — distinct per occurrence.
    let recurrence_ids: Vec<Option<String>> = snapshot
        .events
        .iter()
        .map(|e| e.recurrence_id.clone())
        .collect();
    assert_eq!(
        recurrence_ids,
        vec![
            Some("series-1@2024-01-08T09:00:00-08:00".to_string()),
            Some("series-1@2024-01-15T09:00:00-08:00".to_string()),
            Some("series-1@2024-01-22T09:00:00-08:00".to_string()),
        ]
    );
    assert!(snapshot
        .events
        .iter()
        .all(|e| e.status == EventStatus::Confirmed));
}

#[tokio::test]
async fn cancelled_instance_in_a_series_is_mapped_not_dropped() {
    let transport =
        FixtureTransport::single_calendar("cal-primary", vec![Some(fixture("cancelled_instance"))]);

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 3);
    let cancelled = snapshot
        .events
        .iter()
        .find(|e| e.provider_event_id == "series-2_20240115T170000Z")
        .expect("cancelled instance present in the snapshot");

    assert_eq!(cancelled.status, EventStatus::Cancelled);
    assert_eq!(
        cancelled.recurrence_id.as_deref(),
        Some("series-2@2024-01-15T09:00:00-08:00")
    );
    // No start/end came from Google for a cancelled instance: the adapter
    // falls back to originalStartTime for both boundaries.
    assert_eq!(cancelled.start, cancelled.end);
    assert_eq!(cancelled.title, "");

    let confirmed_neighbors = snapshot
        .events
        .iter()
        .filter(|e| e.status == EventStatus::Confirmed)
        .count();
    assert_eq!(confirmed_neighbors, 2);
}

#[tokio::test]
async fn all_day_boundaries_map_to_utc_midnight_with_exclusive_multi_day_end() {
    let transport =
        FixtureTransport::single_calendar("cal-primary", vec![Some(fixture("all_day_boundaries"))]);

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 2);

    let holiday = &snapshot.events[0];
    assert!(holiday.all_day);
    assert_eq!(holiday.start.time_zone, "");
    assert_eq!(holiday.end.time_zone, "");

    let conference = &snapshot.events[1];
    assert!(conference.all_day);
    // Exclusive end: the offsite runs Mar 1-3, end date is Mar 4.
    let span_ms = conference.end.instant_ms - conference.start.instant_ms;
    assert_eq!(span_ms, 3 * 24 * 60 * 60 * 1000);
}

#[tokio::test]
async fn dst_transition_day_produces_real_elapsed_instants_not_wall_clock_offsets() {
    let transport =
        FixtureTransport::single_calendar("cal-primary", vec![Some(fixture("dst_transition"))]);

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 2);
    let early = &snapshot.events[0];
    let post_jump = &snapshot.events[1];

    // 01:45-08:00 -> 09:45Z; 03:30-07:00 -> 10:30Z: 45 real minutes apart,
    // even though the wall-clock gap reads as 1h45m across the spring
    // forward.
    let gap_ms = post_jump.start.instant_ms - early.end.instant_ms;
    assert_eq!(gap_ms, 45 * 60 * 1000);
    assert_eq!(early.start.time_zone, "America/Los_Angeles");

    let expected_early_start = EventTime::timed(
        chrono::DateTime::parse_from_rfc3339("2024-03-10T09:30:00Z")
            .unwrap()
            .timestamp_millis(),
        "America/Los_Angeles",
    );
    assert_eq!(early.start, expected_early_start);
}

#[tokio::test]
async fn multi_page_pagination_assembles_every_page_of_every_calendar() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![
            Some(fixture("pagination_page_1")),
            Some(fixture("pagination_page_2")),
        ],
    );

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 3);
    let ids: Vec<&str> = snapshot
        .events
        .iter()
        .map(|e| e.provider_event_id.as_str())
        .collect();
    assert_eq!(ids, vec!["page1-evt-1", "page1-evt-2", "page2-evt-1"]);
}

#[tokio::test]
async fn mid_pagination_failure_yields_no_snapshot_at_all() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![Some(fixture("pagination_page_1")), None],
    );

    let result =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS).await;

    assert!(
        result.is_err(),
        "a page 2 transport failure must abort the whole snapshot, never return page 1's events alone"
    );
}
