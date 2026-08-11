//! [`CalendarHostCore`]: the web host's one door into #72's `ContextPoller`
//! over #71's Google adapter (issue #73), kept free of `wasm_bindgen` so it
//! is testable with plain `cargo test` on any target — `lib.rs`'s
//! `wasm_bindings` module is the thin JS-facing shim over this.

use hummingbird_core::calendar::google::{
    CalendarListEntry, GoogleProviderPoller, ReqwestGoogleTransport,
};
use hummingbird_core::calendar::CalendarSelection;
use hummingbird_core::calendar::{events_overlapping_interval, CalendarSnapshot, EventRecord, Interval};
use hummingbird_core::context::{ContextPoller, CredentialEvent, PollOutcome};
use hummingbird_core::freshness::Freshness;
use hummingbird_core::storage::Envelope;

// The snapshot store: real IndexedDB in the browser, in-memory on any other
// target (only reached by `cargo test --workspace`, which never touches
// `wasm32`-gated code — see `client/core/src/storage/mod.rs`).
#[cfg(target_arch = "wasm32")]
type StoreImpl = hummingbird_core::storage::IndexedDbSnapshotStore;
#[cfg(not(target_arch = "wasm32"))]
type StoreImpl = hummingbird_core::storage::MemorySnapshotStore;

fn new_store(namespace: &str) -> StoreImpl {
    #[cfg(target_arch = "wasm32")]
    {
        StoreImpl::new(namespace)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = namespace;
        StoreImpl::default()
    }
}

type GooglePoller = ContextPoller<GoogleProviderPoller<ReqwestGoogleTransport>, StoreImpl>;

const SCHEMA_VERSION: u32 = 1;
const PROVIDER: &str = "google_calendar";

/// The calendar lane's declared poll cadence (#46/ADR-0005's 15-minute
/// foreground timer), passed into [`Freshness::measure`] exactly as
/// `freshness.rs`'s module doc describes: this lane has no
/// [`hummingbird_domain::SnapshotEnvelope`] to read a cadence from, so the
/// host supplies the same constant it drives the timer with. Must match
/// `client/web/src/shell/useCalendarWiring.ts`'s `TIMER_INTERVAL_MS` — the
/// same "kept in step by doc comment, not by the type system" contract
/// `city-waste`'s `polled_every_ms` carries for its cron.
pub const CALENDAR_POLL_INTERVAL_MS: i64 = 15 * 60 * 1000;

/// The answer to an [`events_in_interval`](CalendarHostCore::events_in_interval)
/// read: three states, not two, per issue #267's acceptance criteria.
///
/// `"not_read"` and `"read"` with an empty `events` are deliberately
/// distinct — a device that has never synced this calendar has nothing to
/// say about "the next hour", while a device that has synced and genuinely
/// has nothing scheduled has answered "no events" for real. Collapsing them
/// would make a standing question that reads this arm render "nothing on"
/// for a device that has simply never connected, exactly the false-quiet
/// reading ADR-0015 rules out everywhere else. `"busy"` (the core was
/// checked out for another call) is a third, wasm-only state added by
/// `wasm_bindings::CalendarHost::events_in_interval` — this type never
/// produces it itself, since a plain `&self` read is never checked out.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarEventsResponse {
    pub kind: &'static str,
    /// Every non-cancelled event overlapping the requested interval, in the
    /// query's own deterministic order — never re-sorted here. Empty for
    /// `"not_read"` and `"busy"`.
    pub events: Vec<EventRecord>,
    /// The snapshot's age, computed here rather than a fetch stamp handed
    /// over for the caller to subtract from (ADR-0015's clock rule).
    /// `None` for `"not_read"` and `"busy"` — there is no snapshot to
    /// measure.
    pub freshness: Option<Freshness>,
}

/// The response shape for `listCalendars` (issue #73's picker options).
///
/// A failure is reported as a `kind`, not thrown: the option list is a UX
/// nicety and never a poll dependency, so the host's job on a bad list is to
/// leave the picker as it stands, not to surface an error.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarListResponse {
    /// `"ok"`, `"no_credential"` (nothing pushed yet, or polling is held on
    /// a rejected token), or `"failed"`.
    pub kind: &'static str,
    pub calendars: Vec<CalendarListEntry>,
}

/// Plain-Rust wrapper over one Google Calendar [`ContextPoller`], holding
/// exactly the operations the web host needs.
pub struct CalendarHostCore {
    poller: GooglePoller,
}

impl CalendarHostCore {
    pub fn new(namespace: String, selections: Vec<CalendarSelection>) -> Self {
        let store = new_store(&namespace);
        let transport = ReqwestGoogleTransport::default();
        let fetcher = GoogleProviderPoller::new(transport, selections);
        let poller = ContextPoller::new(PROVIDER, fetcher, store, SCHEMA_VERSION);
        Self { poller }
    }

    pub fn push_token(&mut self, token: String) {
        self.poller.push_token(token);
    }

    /// The picker's current selection (#121: each entry carries its own poll
    /// horizon). Takes effect on the next poll trigger.
    pub fn set_calendar_selections(&self, selections: Vec<CalendarSelection>) {
        self.poller.fetcher().set_calendar_selections(selections);
    }

    pub async fn start(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.start(now_ms).await
    }

    pub async fn refresh(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.refresh(now_ms).await
    }

    pub async fn on_timer(&mut self, now_ms: i64) -> PollOutcome {
        self.poller.on_timer(now_ms).await
    }

    /// The calendars this device's credential can read — the picker's
    /// options. Uses the token already pushed for polling rather than taking
    /// one: the host has no reason to hand the same credential across the
    /// boundary twice, and a token the core is holding on must not go out.
    pub async fn list_calendars(&self) -> CalendarListResponse {
        let Some(token) = self.poller.current_token() else {
            return CalendarListResponse {
                kind: "no_credential",
                calendars: Vec::new(),
            };
        };
        match self.poller.fetcher().list_calendars(&token).await {
            Ok(calendars) => CalendarListResponse {
                kind: "ok",
                calendars,
            },
            Err(_) => CalendarListResponse {
                kind: "failed",
                calendars: Vec::new(),
            },
        }
    }

    pub fn take_credential_events(&mut self) -> Vec<CredentialEvent> {
        self.poller.take_credential_events()
    }

    /// Issue #267: every non-cancelled event overlapping `[start_ms,
    /// end_ms)`, plus how old the answer is — the read side of this poller's
    /// mirror, alongside its lifecycle triggers.
    ///
    /// **Reuses the core's own query and predicate, never a second copy.**
    /// [`events_overlapping_interval`] already filters cancelled instances
    /// (via `hummingbird_core::calendar::is_actionable`, `pub(crate)` there
    /// and so not linkable from this crate) and returns them in its own
    /// deterministic order; this method neither re-filters nor re-sorts.
    /// The interval is the caller's, never sampled here — the query's
    /// existing determinism is preserved exactly.
    ///
    /// `"not_read"` vs `"read"` with an empty `events` is the whole reason
    /// this returns [`CalendarEventsResponse`] rather than a bare
    /// `Vec<EventRecord>`: a device that has never synced has nothing to
    /// say, which is a different fact from "synced, and empty".
    pub async fn events_in_interval(
        &self,
        start_ms: i64,
        end_ms: i64,
        now_ms: i64,
    ) -> CalendarEventsResponse {
        let snapshot = self.poller.current_snapshot().await;
        build_events_response(snapshot, start_ms, end_ms, now_ms)
    }
}

/// The pure half of [`CalendarHostCore::events_in_interval`], split out so it
/// is testable without a real Google fetch: [`GooglePoller`] is hardwired to
/// [`ReqwestGoogleTransport`], so there is no way to script a snapshot into
/// `CalendarHostCore` itself in a native test. Every decision — the
/// `"not_read"` vs `"read"` split, the interval query, the freshness
/// calculation — lives here; [`CalendarHostCore::events_in_interval`] is
/// only the I/O that produces this function's input.
fn build_events_response(
    snapshot: Option<Envelope<CalendarSnapshot>>,
    start_ms: i64,
    end_ms: i64,
    now_ms: i64,
) -> CalendarEventsResponse {
    let Some(envelope) = snapshot else {
        return CalendarEventsResponse {
            kind: "not_read",
            events: Vec::new(),
            freshness: None,
        };
    };
    let events = events_overlapping_interval(&envelope.payload, Interval::new(start_ms, end_ms))
        .into_iter()
        .cloned()
        .collect();
    let freshness = Freshness::measure(
        now_ms,
        Some(envelope.as_of as i64),
        Some(CALENDAR_POLL_INTERVAL_MS),
    );
    CalendarEventsResponse {
        kind: "read",
        events,
        freshness: Some(freshness),
    }
}

/// Maps a [`PollOutcome`] to the stable string name the web host's protocol
/// (`client/web/src/store/protocol.ts`) matches on.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn outcome_name(outcome: PollOutcome) -> &'static str {
    match outcome {
        PollOutcome::NoCredential => "no_credential",
        PollOutcome::Held => "held",
        PollOutcome::Succeeded => "succeeded",
        PollOutcome::TransientFailure => "transient_failure",
        PollOutcome::Unauthorized => "unauthorized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_core::calendar::{EventStatus, EventTime};

    fn timed_event(id: &str, start_ms: i64, end_ms: i64) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: id.to_string(),
            start: EventTime::timed(start_ms, "America/Los_Angeles"),
            end: EventTime::timed(end_ms, "America/Los_Angeles"),
            all_day: false,
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: start_ms,
            html_link: None,
        }
    }

    fn cancelled_event(id: &str, start_ms: i64) -> EventRecord {
        EventRecord {
            status: EventStatus::Cancelled,
            ..timed_event(id, start_ms, start_ms)
        }
    }

    // -- build_events_response (the pure half) --------------------------

    #[test]
    fn no_snapshot_at_all_is_not_read_not_an_empty_read() {
        let response = build_events_response(None, 0, 10_000, 5_000);
        assert_eq!(
            response,
            CalendarEventsResponse { kind: "not_read", events: Vec::new(), freshness: None }
        );
    }

    #[test]
    fn a_snapshot_with_nothing_overlapping_is_a_real_read_not_not_read() {
        let snapshot = CalendarSnapshot::new(vec![timed_event("far-away", 100_000, 101_000)]);
        let envelope = Envelope::new(1, 1_000, snapshot);

        let response = build_events_response(Some(envelope), 0, 10_000, 1_000);

        assert_eq!(response.kind, "read");
        assert_eq!(response.events, Vec::new());
        assert!(response.freshness.is_some());
    }

    #[test]
    fn overlapping_events_come_back_in_the_querys_own_order_cancelled_excluded() {
        let snapshot = CalendarSnapshot::new(vec![
            timed_event("afternoon", 3_000, 4_000),
            cancelled_event("cancelled-standup", 1_500),
            timed_event("morning", 1_000, 2_000),
        ]);
        let envelope = Envelope::new(1, 1_000, snapshot);

        let response = build_events_response(Some(envelope), 0, 5_000, 1_000);

        let ids: Vec<&str> = response
            .events
            .iter()
            .map(|event| event.provider_event_id.as_str())
            .collect();
        assert_eq!(ids, vec!["morning", "afternoon"]);
    }

    #[test]
    fn freshness_is_measured_against_the_snapshots_own_as_of_with_the_calendar_cadence() {
        let snapshot = CalendarSnapshot::new(vec![]);
        let envelope = Envelope::new(1, 1_000, snapshot);

        let response = build_events_response(Some(envelope), 0, 1, 61_000);

        assert_eq!(
            response.freshness,
            Some(Freshness::Age { age_ms: 60_000, declared_cadence_ms: Some(CALENDAR_POLL_INTERVAL_MS) })
        );
    }

    #[test]
    fn an_all_day_events_instant_and_zone_both_cross_the_seam_untouched() {
        // Issue #267's acceptance: a consumer must be able to recover the
        // civil date from the DTO alone, which needs BOTH the instant and
        // the zone it was resolved in — never a device-local flattening.
        let day_ms = 24 * 60 * 60 * 1000;
        let aug_10_start = 19_579 * day_ms;
        let event = EventRecord {
            all_day: true,
            start: EventTime::all_day(aug_10_start, "Pacific/Auckland"),
            end: EventTime::all_day(aug_10_start + day_ms, "Pacific/Auckland"),
            ..timed_event("holiday", aug_10_start, aug_10_start + day_ms)
        };
        let snapshot = CalendarSnapshot::new(vec![event]);
        let envelope = Envelope::new(1, 1_000, snapshot);

        let response =
            build_events_response(Some(envelope), aug_10_start, aug_10_start + day_ms, 1_000);

        assert_eq!(response.events.len(), 1);
        let start = &response.events[0].start;
        assert_eq!(start.instant_ms, aug_10_start);
        assert_eq!(start.time_zone, "Pacific/Auckland");
    }

    #[test]
    fn the_wire_shape_is_kind_events_freshness_never_a_flattened_shape() {
        let json = serde_json::to_string(&CalendarEventsResponse {
            kind: "not_read",
            events: Vec::new(),
            freshness: None,
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"not_read","events":[],"freshness":null}"#);
    }

    #[test]
    fn a_full_read_serializes_every_key_calendar_worker_ts_names_in_raw_calendar_event() {
        // Nothing mechanical connects this serde output to
        // `calendar-worker.ts`'s hand-written `RawCalendarEvent` /
        // `RawFreshness` — a rename on either side compiles and passes on
        // both, the exact gap `server/city-waste/tests/contract.rs` closes
        // for its own body. This pins the literal snake_case keys against
        // the TypeScript's own text, for a `"read"` carrying one timed and
        // one all-day event (the `"not_read"` case is already pinned above).
        let timed = timed_event("morning", 1_000, 2_000);
        let all_day = EventRecord {
            all_day: true,
            start: EventTime::all_day(86_400_000, "Pacific/Auckland"),
            end: EventTime::all_day(172_800_000, "Pacific/Auckland"),
            ..timed_event("holiday", 86_400_000, 172_800_000)
        };
        let response = CalendarEventsResponse {
            kind: "read",
            events: vec![timed, all_day],
            freshness: Some(Freshness::Age {
                age_ms: 60_000,
                declared_cadence_ms: Some(CALENDAR_POLL_INTERVAL_MS),
            }),
        };

        let json = serde_json::to_value(&response).unwrap();

        for event in json["events"].as_array().unwrap() {
            for key in [
                "provider_event_id",
                "calendar_id",
                "title",
                "start",
                "end",
                "all_day",
                "recurrence_id",
                "location",
                "organizer",
                "status",
                "provider_updated_at_ms",
                "html_link",
            ] {
                assert!(event.get(key).is_some(), "event missing key {key:?}: {event}");
            }
            for boundary in [&event["start"], &event["end"]] {
                for key in ["instant_ms", "time_zone"] {
                    assert!(
                        boundary.get(key).is_some(),
                        "boundary missing key {key:?}: {boundary}"
                    );
                }
            }
        }

        assert_eq!(json["freshness"]["state"], "age");
        for key in ["age_ms", "declared_cadence_ms"] {
            assert!(
                json["freshness"].get(key).is_some(),
                "freshness missing key {key:?}: {}",
                json["freshness"]
            );
        }
    }

    #[tokio::test]
    async fn a_fresh_host_has_no_credential_events() {
        let mut host = CalendarHostCore::new("test-ns".to_string(), vec![CalendarSelection::standard("primary")]);

        assert_eq!(host.take_credential_events(), Vec::new());
    }

    #[tokio::test]
    async fn a_never_polled_host_answers_events_in_interval_not_read() {
        // The real async method, end to end (no scripted snapshot possible
        // here — `GooglePoller` is hardwired to `ReqwestGoogleTransport` —
        // but this is the one state reachable without a network call, and it
        // proves the wiring from `current_snapshot()` through to the pure
        // `build_events_response` above.
        let host = CalendarHostCore::new("test-ns".to_string(), vec![CalendarSelection::standard("primary")]);

        let response = host.events_in_interval(0, 1_000, 1_000).await;

        assert_eq!(
            response,
            CalendarEventsResponse { kind: "not_read", events: Vec::new(), freshness: None }
        );
    }

    #[tokio::test]
    async fn start_with_no_pushed_token_reports_no_credential() {
        let mut host = CalendarHostCore::new("test-ns".to_string(), vec![CalendarSelection::standard("primary")]);
        let outcome = host.start(1_000).await;
        assert_eq!(outcome_name(outcome), "no_credential");
    }

    #[tokio::test]
    async fn listing_calendars_before_any_token_is_pushed_reports_no_credential() {
        // The one branch reachable without a network: it matters because the
        // picker calls this on a device whose silent re-mint failed, and a
        // "no_credential" answer must leave the existing options alone
        // rather than clearing them.
        let host = CalendarHostCore::new("test-ns".to_string(), vec![CalendarSelection::standard("primary")]);

        assert_eq!(
            host.list_calendars().await,
            CalendarListResponse {
                kind: "no_credential",
                calendars: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn set_calendar_selections_is_readable_back_through_the_wrapped_fetcher() {
        let host = CalendarHostCore::new(
            "test-ns".to_string(),
            vec![CalendarSelection::standard("a")],
        );
        host.set_calendar_selections(vec![
            CalendarSelection::standard("b"),
            CalendarSelection::long("c"),
        ]);
        assert_eq!(
            host.poller.fetcher().calendar_selections(),
            vec![CalendarSelection::standard("b"), CalendarSelection::long("c")]
        );
    }

    #[tokio::test]
    async fn a_selection_list_crosses_the_wasm_seam_as_json_text() {
        // `lib.rs`'s `setCalendarSelections` takes one JSON string rather
        // than positional arguments, the same shape `TriageEdits` uses: a
        // `Vec<String>` cannot carry a per-entry horizon, and a parallel
        // second array would let the two drift by length.
        let parsed: Vec<CalendarSelection> =
            serde_json::from_str(r#"[{"id":"primary","horizon":"standard"},{"id":"trips","horizon":"long"}]"#)
                .unwrap();
        assert_eq!(
            parsed,
            vec![
                CalendarSelection::standard("primary"),
                CalendarSelection::long("trips")
            ]
        );
    }
}
