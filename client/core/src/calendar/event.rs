//! [`EventRecord`]: the provider-agnostic calendar event shape (issue #46's
//! field list), and its supporting types.

use serde::{Deserialize, Serialize};

/// *When* an event happens — two arms, discriminated by whether it is an
/// all-day event, per ADR-0015's 2026-08-10 amendment.
///
/// **An all-day event carries civil dates and nothing else; a timed event
/// carries UTC instants and nothing else. Neither arm stores a source time
/// zone.** An all-day event is a fact about a *day*, not about an instant:
/// flattening it to zone-resolved midnight and reading that instant back in
/// another zone lands a calendar day out — the "India in 394 days" defect
/// ADR-0015 records on #121. The two-armed shape makes both forbidden
/// states (an all-day instant, a stored zone) unrepresentable rather than
/// merely discouraged.
///
/// The all-day arm keeps the provider's own strings **byte-identical**,
/// including its exclusive-end convention (`end_date` is the day *after*
/// the last day). Membership questions over it are lexicographic
/// `YYYY-MM-DD` comparisons, which need no time zone at all; resolving a
/// civil date to an instant is the *reader's* job, in the reader's own zone,
/// and this crate — bare `wasm32`, no tzdb — never does it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventWhen {
    /// Civil dates, `YYYY-MM-DD`, exactly as the provider stated them.
    /// `end_date` is exclusive.
    AllDay {
        start_date: String,
        end_date: String,
    },
    /// Milliseconds since the Unix epoch, UTC. `end_ms` is exclusive.
    Timed { start_ms: i64, end_ms: i64 },
}

impl EventWhen {
    /// A timed span: two UTC instants.
    pub fn timed(start_ms: i64, end_ms: i64) -> Self {
        Self::Timed { start_ms, end_ms }
    }

    /// An all-day span: the provider's civil dates, exclusive end.
    pub fn all_day(start_date: impl Into<String>, end_date: impl Into<String>) -> Self {
        Self::AllDay {
            start_date: start_date.into(),
            end_date: end_date.into(),
        }
    }

    pub fn is_all_day(&self) -> bool {
        matches!(self, Self::AllDay { .. })
    }
}

/// The provider's confirmation state for an event.
///
/// Deliberately generic (not `GoogleEventStatus`): #47's future M365 adapter
/// targets this same enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

/// One calendar event instance, provider-agnostic throughout.
///
/// Carries issue #46's full field list: provider event id, calendar id,
/// title, when it happens ([`EventWhen`], which subsumes the old
/// start/end/all-day trio), recurrence identity, location, organizer,
/// status, provider update time, and HTML link. No field or name here is
/// Google-specific — this is the shared shape #47's M365 adapter later
/// fills too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    /// The provider's own id for this expanded instance (recurring events
    /// expand to one id per instance).
    pub provider_event_id: String,
    /// The provider's id for the calendar this event belongs to.
    pub calendar_id: String,
    pub title: String,
    pub when: EventWhen,
    /// Identifies which recurring series (and which instance of it) this
    /// event belongs to, or `None` for a non-recurring event.
    pub recurrence_id: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub status: EventStatus,
    /// Milliseconds since the Unix epoch, UTC: when the provider last
    /// updated this event.
    pub provider_updated_at_ms: i64,
    pub html_link: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> EventRecord {
        EventRecord {
            provider_event_id: "evt-1".to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "Standup".to_string(),
            when: EventWhen::timed(1_700_000_000_000, 1_700_000_600_000),
            recurrence_id: Some("series-1".to_string()),
            location: Some("Zoom".to_string()),
            organizer: Some("john@twinion.net".to_string()),
            status: EventStatus::Confirmed,
            provider_updated_at_ms: 1_699_999_000_000,
            html_link: Some("https://calendar.google.com/event?eid=abc".to_string()),
        }
    }

    #[test]
    fn event_record_round_trips_through_serde_json_with_every_field_intact() {
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let restored: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, event);
    }

    #[test]
    fn an_all_day_event_round_trips_with_its_dates_byte_identical() {
        let event = EventRecord {
            when: EventWhen::all_day("2026-09-09", "2026-09-16"),
            ..sample_event()
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, event);
        assert_eq!(
            restored.when,
            EventWhen::AllDay {
                start_date: "2026-09-09".to_string(),
                end_date: "2026-09-16".to_string(),
            }
        );
    }

    #[test]
    fn the_two_arms_are_internally_tagged_on_kind_so_the_wire_is_a_discriminated_union() {
        // Internally tagged so the seam's DTO can be a discriminated
        // union at all; an externally-tagged or untagged enum would not
        // map onto one. The tag's *spelling* is not the DTO's: this crate
        // serializes snake_case like every other tagged enum crossing
        // this seam (`freshness.rs`, `rank.rs`, `bindings.rs`,
        // `pane.rs`), and `calendar-worker.ts`'s `mapCalendarEventWhen`
        // is the one place `all_day` becomes `store/protocol.ts`'s
        // `allDay` — the same rename `mapFreshness` already does.
        let timed = serde_json::to_value(EventWhen::timed(1_000, 2_000)).unwrap();
        assert_eq!(timed["kind"], "timed");
        assert_eq!(timed["start_ms"], 1_000);
        assert_eq!(timed["end_ms"], 2_000);

        let all_day = serde_json::to_value(EventWhen::all_day("2026-09-09", "2026-09-16")).unwrap();
        assert_eq!(all_day["kind"], "all_day");
        assert_eq!(all_day["start_date"], "2026-09-09");
        assert_eq!(all_day["end_date"], "2026-09-16");
    }
}
