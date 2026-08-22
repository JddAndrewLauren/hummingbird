//! [`CalendarSnapshot`]: the persisted set of expanded event instances for
//! the mirror's rolling window.

use serde::{Deserialize, Serialize};

use crate::storage::{Persistable, PersistableSealed};

use super::event::EventRecord;

/// A snapshot of expanded calendar event instances.
///
/// The rolling −7d/+90d window (issue #46) is a policy the adapter (#71)
/// enforces when it builds a snapshot to save — this type itself holds
/// whatever instances it is given. Persistence (atomic write, `as_of`) comes
/// from #68's [`crate::storage::save_snapshot`]/[`crate::storage::load_snapshot`],
/// which this type opts into via [`Persistable`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarSnapshot {
    pub events: Vec<EventRecord>,
}

impl PersistableSealed for CalendarSnapshot {}
impl Persistable for CalendarSnapshot {}

impl CalendarSnapshot {
    pub fn new(events: Vec<EventRecord>) -> Self {
        Self { events }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::event::{EventStatus, EventWhen};
    use crate::storage::{
        load_snapshot, load_snapshot_at_version, save_snapshot, MemorySnapshotStore,
    };

    fn one_event(id: &str) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "Standup".to_string(),
            when: EventWhen::timed(1_700_000_000_000, 1_700_000_600_000),
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: 1_699_999_000_000,
            html_link: None,
            description: None,
        }
    }

    #[tokio::test]
    async fn snapshot_persists_atomically_and_reads_back_with_as_of_intact() {
        let store = MemorySnapshotStore::default();
        let snapshot = CalendarSnapshot::new(vec![one_event("evt-1"), one_event("evt-2")]);

        save_snapshot(&store, 1, 1_700_000_000_000, &snapshot)
            .await
            .unwrap();

        let loaded: CalendarSnapshot = load_snapshot(&store).await.unwrap().unwrap().payload;
        assert_eq!(loaded, snapshot);
    }

    #[tokio::test]
    async fn as_of_survives_the_round_trip_alongside_the_payload() {
        let store = MemorySnapshotStore::default();
        let snapshot = CalendarSnapshot::new(vec![one_event("evt-1")]);

        save_snapshot(&store, 3, 1_700_000_555_000, &snapshot)
            .await
            .unwrap();

        let envelope = load_snapshot::<CalendarSnapshot, _>(&store)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(envelope.as_of, 1_700_000_555_000);
        assert_eq!(envelope.schema_version, 3);
        assert_eq!(envelope.payload, snapshot);
    }

    // -- the #46 migration ---------------------------------------------
    //
    // ADR-0015's 2026-08-10 amendment replaced this payload's per-event
    // `start`/`end`/`all_day` with one two-armed `when`, and
    // `ffi-web`'s `CalendarHostCore::SCHEMA_VERSION` went 1 -> 2 for it.
    // The migration is version-discard, and the two tests below are the
    // whole of why that works: the version is decided BEFORE the payload
    // is parsed, and the payload genuinely no longer parses.

    /// The v1 payload, exactly as a device that has not updated still has
    /// it on disk — the old field trio, and a zone on each boundary.
    #[derive(Debug, Serialize, Deserialize)]
    struct V1CalendarSnapshot {
        events: Vec<V1Event>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct V1Event {
        provider_event_id: String,
        calendar_id: String,
        title: String,
        start: V1EventTime,
        end: V1EventTime,
        all_day: bool,
        recurrence_id: Option<String>,
        location: Option<String>,
        organizer: Option<String>,
        status: String,
        provider_updated_at_ms: i64,
        html_link: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct V1EventTime {
        instant_ms: i64,
        time_zone: String,
    }

    impl PersistableSealed for V1CalendarSnapshot {}
    impl Persistable for V1CalendarSnapshot {}

    async fn store_holding_a_v1_snapshot() -> MemorySnapshotStore {
        let store = MemorySnapshotStore::default();
        let v1 = V1CalendarSnapshot {
            events: vec![V1Event {
                provider_event_id: "holiday-1".to_string(),
                calendar_id: "cal-primary".to_string(),
                title: "New Year's Day".to_string(),
                // The flattening itself: local midnight resolved in the
                // calendar's zone, which is what the amendment deleted.
                start: V1EventTime {
                    instant_ms: 1_704_096_000_000,
                    time_zone: "America/Los_Angeles".to_string(),
                },
                end: V1EventTime {
                    instant_ms: 1_704_182_400_000,
                    time_zone: "America/Los_Angeles".to_string(),
                },
                all_day: true,
                recurrence_id: None,
                location: None,
                organizer: None,
                status: "confirmed".to_string(),
                provider_updated_at_ms: 0,
                html_link: None,
            }],
        };
        save_snapshot(&store, 1, 1_700_000_000_000, &v1)
            .await
            .unwrap();
        store
    }

    #[tokio::test]
    async fn a_v1_calendar_snapshot_is_discarded_at_v2_rather_than_parsed() {
        // What a device that updates across #46 actually does on its first
        // read: nothing polled yet, one poll interval of `not_read`, then
        // a fresh mirror. A context mirror is disposable — it holds
        // nothing this device authored — which is what makes discarding
        // the right answer here and NOT the right answer for the task
        // mirror's own queue.
        let store = store_holding_a_v1_snapshot().await;

        let loaded = load_snapshot_at_version::<CalendarSnapshot, _>(&store, 2)
            .await
            .unwrap();

        assert_eq!(loaded, None);
    }

    #[tokio::test]
    async fn the_version_blind_load_errors_on_that_same_v1_snapshot() {
        // The other half, and the reason `current_snapshot` had to move off
        // `load_snapshot`: this payload does not deserialize as the new
        // type at all, so a version-blind read fails BEFORE any caller's
        // own `schema_version` check could discard it — bricking the read
        // on every poll until storage is cleared by hand. The ordering is
        // the migration.
        let store = store_holding_a_v1_snapshot().await;

        assert!(load_snapshot::<CalendarSnapshot, _>(&store).await.is_err());
    }
}
