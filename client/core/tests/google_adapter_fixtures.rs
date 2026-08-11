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
use hummingbird_core::calendar::{EventStatus, EventWhen};

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
        "all_day_east_of_utc" => {
            include_str!("fixtures/google/all_day_east_of_utc.json").to_string()
        }
        "dst_transition" => include_str!("fixtures/google/dst_transition.json").to_string(),
        "pagination_page_1" => include_str!("fixtures/google/pagination_page_1.json").to_string(),
        "pagination_page_2" => include_str!("fixtures/google/pagination_page_2.json").to_string(),
        other => panic!("unknown fixture {other}"),
    }
}

/// One scripted `(expected_page_token, response)` pair. A `None` response
/// stands in for a transport failure.
type FixturePage = (Option<String>, Option<String>);

/// Serves a scripted sequence of pages per calendar id, popped in call
/// order. The expected token is asserted against what the adapter actually
/// passes, so a fixture transport that ignored pagination and re-requested
/// page 1 forever would fail the test instead of silently passing.
struct FixtureTransport {
    pages: Mutex<HashMap<String, Vec<FixturePage>>>,
}

impl FixtureTransport {
    fn single_calendar(calendar_id: &str, pages: Vec<FixturePage>) -> Self {
        let mut map = HashMap::new();
        map.insert(calendar_id.to_string(), pages);
        Self {
            pages: Mutex::new(map),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl EventsTransport for FixtureTransport {
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
            .unwrap_or_else(|| panic!("no fixture pages scripted for calendar {calendar_id}"));
        if queue.is_empty() {
            panic!("fixture transport ran out of pages for calendar {calendar_id}");
        }
        let (expected_token, response) = queue.remove(0);
        assert_eq!(
            expected_token.as_deref(),
            page_token,
            "calendar {calendar_id}: adapter requested page_token {page_token:?}, expected {expected_token:?}"
        );
        match response {
            Some(body) => Ok(body),
            None => Err(TransportError::new("simulated transport failure")),
        }
    }
}

#[tokio::test]
async fn recurrence_expansion_yields_one_instance_per_series_occurrence() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![(None, Some(fixture("recurrence_expansion")))],
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
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![(None, Some(fixture("cancelled_instance")))],
    );

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
    // falls back to originalStartTime for both boundaries, which makes the
    // span zero-length whichever arm it lands on.
    assert_eq!(
        cancelled.when,
        EventWhen::Timed {
            start_ms: chrono::DateTime::parse_from_rfc3339("2024-01-15T09:00:00-08:00")
                .unwrap()
                .timestamp_millis(),
            end_ms: chrono::DateTime::parse_from_rfc3339("2024-01-15T09:00:00-08:00")
                .unwrap()
                .timestamp_millis(),
        }
    );
    assert_eq!(cancelled.title, "");

    let confirmed_neighbors = snapshot
        .events
        .iter()
        .filter(|e| e.status == EventStatus::Confirmed)
        .count();
    assert_eq!(confirmed_neighbors, 2);
}

#[tokio::test]
async fn all_day_boundaries_keep_the_providers_own_dates_and_exclusive_end() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![(None, Some(fixture("all_day_boundaries")))],
    );

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 2);

    // ADR-0015's 2026-08-10 amendment: an all-day event carries civil
    // dates, never instants, and the page's `timeZone` (here
    // America/Los_Angeles) plays no part at all — this used to assert the
    // flattening to 08:00Z that the amendment forbids.
    let holiday = &snapshot.events[0];
    assert_eq!(holiday.when, EventWhen::all_day("2024-01-01", "2024-01-02"));

    // Exclusive end, the provider's own: the offsite runs Mar 1-3 and
    // Google states the end as Mar 4. Nothing normalises it here.
    let conference = &snapshot.events[1];
    assert_eq!(
        conference.when,
        EventWhen::all_day("2024-03-01", "2024-03-04")
    );
}

#[tokio::test]
async fn an_all_day_event_on_a_calendar_east_of_utc_is_byte_identical_to_the_fixture() {
    // The case ADR-0015's amendment is named after: a week in India
    // (2026-09-09 -> 2026-09-16, exclusive) on an Asia/Kolkata calendar.
    // Flattened to zone-resolved midnight instants and read back anywhere
    // west, this event starts a calendar day early — "India in 394 days".
    // The dates that come out are the dates that went in, and there is no
    // zone anywhere in the mapped record to read them against.
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![(None, Some(fixture("all_day_east_of_utc")))],
    );

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 1);
    let trip = &snapshot.events[0];
    assert_eq!(
        trip.when,
        EventWhen::AllDay {
            start_date: "2026-09-09".to_string(),
            end_date: "2026-09-16".to_string(),
        }
    );

    // And the serialized record carries no zone and no instant either —
    // the mirror this writes into is what every reader sees.
    let json = serde_json::to_string(trip).unwrap();
    assert!(!json.contains("Kolkata"), "no source zone survives: {json}");
    assert!(
        json.contains(r#""kind":"all_day""#),
        "the all-day arm is what was stored: {json}"
    );
}

#[tokio::test]
async fn dst_transition_day_produces_real_elapsed_instants_not_wall_clock_offsets() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![(None, Some(fixture("dst_transition")))],
    );

    let snapshot =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS)
            .await
            .expect("complete snapshot");

    assert_eq!(snapshot.events.len(), 2);
    let early = &snapshot.events[0];
    let post_jump = &snapshot.events[1];

    // 01:45-08:00 -> 09:45Z; 03:30-07:00 -> 10:30Z: 45 real minutes apart,
    // even though the wall-clock gap reads as 1h45m across the spring
    // forward. The offset Google sends is all the instant ever needed —
    // no zone database, and none stored.
    let EventWhen::Timed {
        start_ms: post_jump_start,
        ..
    } = post_jump.when
    else {
        panic!("a dateTime boundary maps to the timed arm");
    };
    let EventWhen::Timed {
        start_ms: early_start,
        end_ms: early_end,
    } = early.when
    else {
        panic!("a dateTime boundary maps to the timed arm");
    };
    assert_eq!(post_jump_start - early_end, 45 * 60 * 1000);
    assert_eq!(
        early_start,
        chrono::DateTime::parse_from_rfc3339("2024-03-10T09:30:00Z")
            .unwrap()
            .timestamp_millis()
    );
}

#[tokio::test]
async fn multi_page_pagination_assembles_every_page_of_every_calendar() {
    let transport = FixtureTransport::single_calendar(
        "cal-primary",
        vec![
            (None, Some(fixture("pagination_page_1"))),
            (
                Some("page-2-token".to_string()),
                Some(fixture("pagination_page_2")),
            ),
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
        vec![
            (None, Some(fixture("pagination_page_1"))),
            (Some("page-2-token".to_string()), None),
        ],
    );

    let result =
        fetch_calendar_snapshot(&transport, "token", &["cal-primary".to_string()], NOW_MS).await;

    assert!(
        result.is_err(),
        "a page 2 transport failure must abort the whole snapshot, never return page 1's events alone"
    );
}
