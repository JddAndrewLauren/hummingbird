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
    use crate::calendar::event::{EventStatus, EventTime};
    use crate::storage::{load_snapshot, save_snapshot, MemorySnapshotStore};

    fn one_event(id: &str) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "Standup".to_string(),
            start: EventTime::timed(1_700_000_000_000, "America/Los_Angeles"),
            end: EventTime::timed(1_700_000_600_000, "America/Los_Angeles"),
            all_day: false,
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: 1_699_999_000_000,
            html_link: None,
        }
    }

    #[tokio::test]
    async fn snapshot_persists_atomically_and_reads_back_with_as_of_intact() {
        let store = MemorySnapshotStore::default();
        let snapshot = CalendarSnapshot::new(vec![one_event("evt-1"), one_event("evt-2")]);

        save_snapshot(&store, 1, 1_700_000_000_000, snapshot.clone())
            .await
            .unwrap();

        let loaded: CalendarSnapshot = load_snapshot(&store).await.unwrap().unwrap().payload;
        assert_eq!(loaded, snapshot);
    }

    #[tokio::test]
    async fn as_of_survives_the_round_trip_alongside_the_payload() {
        let store = MemorySnapshotStore::default();
        let snapshot = CalendarSnapshot::new(vec![one_event("evt-1")]);

        save_snapshot(&store, 3, 1_700_000_555_000, snapshot.clone())
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
}
