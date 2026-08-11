//! The stdin contract: one JSON envelope, and the owned→borrowed adapter
//! that rebuilds `rank`'s borrowing calendar types from it.
//!
//! ```jsonc
//! {
//!   "sweep": ChangesResponse,          // GET /api/sweep, verbatim
//!   "axes": {"context": "@computer", "energy": "low", "size": "quick"},
//!   "now":  {"local": "2026-08-11T09:53", "epoch_ms": 1786553580000},
//!   "calendar": {                       // optional; #70's shape verbatim
//!     "current_or_next": {"status": "in_progress", "event": EventRecord},
//!     "today": [EventRecord, ...]
//!   }
//! }
//! ```
//!
//! The calendar block is issue #70's read contract, field-for-field, the
//! same shape `.claude/skills/next-up-personal/SKILL.md` already documents —
//! not a shape invented here, and no provider's field names anywhere in it.
//!
//! [`hummingbird_core::rank::CalendarContext`] and
//! [`hummingbird_core::calendar::CurrentOrNext`] both **borrow** their
//! events, because their host reads them straight out of the device's
//! mirror. The wire cannot, so [`WireCalendar`] owns its records and
//! [`WireCalendar::to_calendar_context`] hands back a view borrowing from
//! it. That adapter is the only place the two representations meet, and it
//! is tested on its own.

use hummingbird_core::calendar::{CurrentOrNext, EventRecord};
use hummingbird_core::rank::{Axes, CalendarContext, Now};
use hummingbird_domain::{ChangesResponse, Energy, Size};
use serde::{Deserialize, Serialize};

/// One envelope, as read from stdin.
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope {
    /// The `GET /api/sweep` response, unchanged. The whole survey is this
    /// one payload — `items`, `blocked_by`, `projects` and `fog` all
    /// arrive typed, so nothing downstream parses prose.
    pub sweep: ChangesResponse,
    /// Every axis is optional and independently skippable; an absent
    /// `axes` key is "no preference on anything", never "match nothing".
    #[serde(default)]
    pub axes: WireAxes,
    pub now: WireNow,
    #[serde(default)]
    pub calendar: Option<WireCalendar>,
}

/// The three declared axes, already parsed — free-text parsing is the
/// skill's job (`SKILL.md`), never this crate's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct WireAxes {
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub energy: Option<Energy>,
    #[serde(default)]
    pub size: Option<Size>,
}

impl WireAxes {
    pub fn to_rank_axes(&self) -> Axes {
        Axes {
            context: self.context.clone(),
            energy: self.energy,
            size: self.size,
        }
    }
}

/// "Now", supplied by the caller in both the shapes `rank` compares
/// against — see [`hummingbird_core::rank::Now`] for why one is not
/// derived from the other.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WireNow {
    /// Naive local, in exactly `Item::deadline`'s own spelling.
    pub local: String,
    pub epoch_ms: i64,
}

impl WireNow {
    pub fn to_rank_now(&self) -> Now {
        Now {
            local: self.local.clone(),
            epoch_ms: self.epoch_ms,
        }
    }
}

/// Issue #70's calendar-context block, owned.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WireCalendar {
    pub current_or_next: WireCurrentOrNext,
    /// Today's events, in local-time order. May be empty — and is read for
    /// exactly one thing, the masked next-start lookup `rank` step 5 needs.
    #[serde(default)]
    pub today: Vec<EventRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WireCurrentOrNext {
    pub status: CalendarStatus,
    /// `null` iff `status` is `none` — the pairing is checked, never
    /// assumed.
    #[serde(default)]
    pub event: Option<EventRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarStatus {
    InProgress,
    Upcoming,
    None,
}

impl CalendarStatus {
    fn as_str(self) -> &'static str {
        match self {
            CalendarStatus::InProgress => "in_progress",
            CalendarStatus::Upcoming => "upcoming",
            CalendarStatus::None => "none",
        }
    }
}

/// What was wrong with the envelope, named. Never a quietly empty answer:
/// a calendar block whose `status` and `event` disagree is a caller bug,
/// and silently reading it as "no calendar" would drop the 30-minute nudge
/// at exactly the moment it matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeProblem {
    /// `status` names an event but `event` is `null`.
    MissingEvent { status: &'static str },
    /// `status` is `none` but an `event` was supplied anyway.
    UnexpectedEvent,
}

impl std::fmt::Display for EnvelopeProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvelopeProblem::MissingEvent { status } => write!(
                f,
                "calendar.current_or_next.status is \"{status}\" but no event was supplied"
            ),
            EnvelopeProblem::UnexpectedEvent => write!(
                f,
                "calendar.current_or_next.status is \"none\" but an event was supplied"
            ),
        }
    }
}

impl std::error::Error for EnvelopeProblem {}

impl WireCalendar {
    /// The owned→borrowed adapter: a [`CalendarContext`] viewing this
    /// block's own records.
    pub fn to_calendar_context(&self) -> Result<CalendarContext<'_>, EnvelopeProblem> {
        let current_or_next = match (self.current_or_next.status, &self.current_or_next.event) {
            (CalendarStatus::InProgress, Some(event)) => CurrentOrNext::InProgress(event),
            (CalendarStatus::Upcoming, Some(event)) => CurrentOrNext::Upcoming(event),
            (CalendarStatus::None, None) => CurrentOrNext::None,
            (CalendarStatus::None, Some(_)) => return Err(EnvelopeProblem::UnexpectedEvent),
            (status, None) => {
                return Err(EnvelopeProblem::MissingEvent {
                    status: status.as_str(),
                })
            }
        };
        Ok(CalendarContext {
            current_or_next,
            today: &self.today,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_core::calendar::{EventStatus, EventWhen};

    fn event(id: &str, start_ms: i64) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "primary".to_string(),
            title: format!("event {id}"),
            when: EventWhen::timed(start_ms, start_ms + 3_600_000),
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: 0,
            html_link: None,
        }
    }

    #[test]
    fn an_envelope_with_only_sweep_and_now_parses_with_every_axis_unset() {
        let envelope: Envelope = serde_json::from_str(
            r#"{"sweep":{"version":0,"projects":[],"routes":[],"fog":[],"items":[],
                "steps":[],"blocked_by":[],"alerts":[],"context_snapshots":[],"settings":[]},
                "now":{"local":"2026-08-11T09:53","epoch_ms":1}}"#,
        )
        .expect("envelope parses");
        assert_eq!(envelope.axes, WireAxes::default());
        assert!(envelope.calendar.is_none());
        assert_eq!(envelope.now.epoch_ms, 1);
    }

    #[test]
    fn axes_parse_the_domain_enums_own_spellings() {
        let axes: WireAxes =
            serde_json::from_str(r#"{"context":"@computer","energy":"low","size":"quick"}"#)
                .expect("axes parse");
        let ranked = axes.to_rank_axes();
        assert_eq!(ranked.context.as_deref(), Some("@computer"));
        assert_eq!(ranked.energy, Some(Energy::Low));
        assert_eq!(ranked.size, Some(Size::Quick));
    }

    #[test]
    fn an_upcoming_block_adapts_onto_a_borrowing_current_or_next() {
        let calendar = WireCalendar {
            current_or_next: WireCurrentOrNext {
                status: CalendarStatus::Upcoming,
                event: Some(event("a", 5_000)),
            },
            today: vec![event("a", 5_000)],
        };
        let context = calendar.to_calendar_context().expect("adapts");
        match context.current_or_next {
            CurrentOrNext::Upcoming(e) => {
                assert_eq!(e.when, EventWhen::timed(5_000, 5_000 + 3_600_000))
            }
            other => panic!("expected Upcoming, got {other:?}"),
        }
        assert_eq!(context.today.len(), 1);
    }

    #[test]
    fn an_in_progress_block_keeps_today_so_the_masked_next_start_is_reachable() {
        let calendar = WireCalendar {
            current_or_next: WireCurrentOrNext {
                status: CalendarStatus::InProgress,
                event: Some(event("standup", 1_000)),
            },
            today: vec![event("standup", 1_000), event("review", 9_000)],
        };
        let context = calendar.to_calendar_context().expect("adapts");
        assert!(matches!(
            context.current_or_next,
            CurrentOrNext::InProgress(_)
        ));
        assert_eq!(context.today.len(), 2);
    }

    #[test]
    fn a_none_status_with_no_event_is_the_empty_calendar() {
        let calendar = WireCalendar {
            current_or_next: WireCurrentOrNext {
                status: CalendarStatus::None,
                event: None,
            },
            today: vec![],
        };
        let context = calendar.to_calendar_context().expect("adapts");
        assert!(matches!(context.current_or_next, CurrentOrNext::None));
    }

    #[test]
    fn a_status_naming_an_event_with_none_supplied_is_a_named_problem() {
        let calendar = WireCalendar {
            current_or_next: WireCurrentOrNext {
                status: CalendarStatus::Upcoming,
                event: None,
            },
            today: vec![],
        };
        assert_eq!(
            calendar.to_calendar_context().unwrap_err(),
            EnvelopeProblem::MissingEvent { status: "upcoming" }
        );
    }

    #[test]
    fn a_none_status_carrying_an_event_is_a_named_problem_too() {
        let calendar = WireCalendar {
            current_or_next: WireCurrentOrNext {
                status: CalendarStatus::None,
                event: Some(event("a", 1)),
            },
            today: vec![],
        };
        assert_eq!(
            calendar.to_calendar_context().unwrap_err(),
            EnvelopeProblem::UnexpectedEvent
        );
    }
}
