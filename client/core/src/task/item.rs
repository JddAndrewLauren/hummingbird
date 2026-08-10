//! [`Item`]: the one record type the mirror holds, and [`Stage`]: where it
//! sits in the funnel.
//!
//! One type with a lifecycle stage rather than separate capture/action types.
//! `CONTEXT.md` defines an **Action** as startable without further planning,
//! which a Triage capture and a Grilling item explicitly are not — but
//! ADR-0001 rule 3 makes the mirror the export of *everything* in team ION,
//! so all of it has to be representable. [`Stage::is_action`] is therefore a
//! predicate over the funnel rather than a type distinction: an Item is the
//! umbrella, an Action is an Item past triage.

use serde::{Deserialize, Serialize};

/// A stable item id.
///
/// This is the Linear issue's UUID, which is also the id the client *mints*
/// for its own captures (ADR-0001: deterministic, client-supplied, v4-shaped
/// — the sweeper's proven idempotency pattern generalized). One id space, so
/// a locally-created item needs no rewrite when the sweep brings it back.
pub type ItemId = String;

/// Where an item sits in the funnel.
///
/// These are the app's own names for the ION workflow states locked in issue
/// #1 (Backlog -> Triage -> Grilling -> Ready -> In Progress -> Blocked ->
/// Done/Canceled). The *mapping* to Linear state ids belongs to the adapter,
/// resolved by name at runtime — no state id appears in this module, which is
/// ADR-0001 seam rule 1 made mechanical.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stage {
    /// Captured, not yet triaged. Pre-Action by definition.
    Triage,
    /// Known to need thinking before it can be planned — `CONTEXT.md`'s fog.
    /// Pre-Action by definition.
    Grilling,
    /// Triaged into a real action, but not selected for the frontier.
    Backlog,
    /// Triaged and selected.
    Ready,
    /// Started.
    InProgress,
    /// An **external wait** — the only meaning of Blocked in this domain
    /// (`CONTEXT.md`). An action waiting on another action is *not* this; it
    /// carries a blocker relation instead, which is why [`Item::blockers`]
    /// exists separately.
    Blocked,
    Done,
    Canceled,
    /// A workflow state this client does not model.
    ///
    /// Deliberately not an error and deliberately not dropped: ADR-0001 rule
    /// 3 says the mirror is the export, so a state added in Linear tomorrow
    /// must still round-trip through the snapshot rather than silently
    /// deleting the items sitting in it.
    Other(String),
}

impl Stage {
    /// Whether this item is an **Action** in `CONTEXT.md`'s sense — startable
    /// without further planning.
    ///
    /// Triage and Grilling are pre-Action by definition. [`Stage::Other`] is
    /// not an Action because this client cannot know that it is: claiming
    /// otherwise would put unrecognized work on the frontier.
    pub fn is_action(&self) -> bool {
        !matches!(self, Stage::Triage | Stage::Grilling | Stage::Other(_))
    }

    /// Whether the item is finished, either way. "Shut" rather than "done"
    /// because Canceled counts: a canceled blocker no longer blocks.
    pub fn is_shut(&self) -> bool {
        matches!(self, Stage::Done | Stage::Canceled)
    }
}

/// Linear's priority, kept as its own type so nothing sorts on the raw
/// number.
///
/// The numeric encoding is inverted *and* has a hole: 0 means "No priority"
/// and sorts after 4/Low rather than before 1/Urgent. A naive
/// `sort_by_key(|i| i.priority)` is therefore wrong twice over, which is why
/// the raw value is not a bare `u8` field on [`Item`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    None,
    Urgent,
    High,
    Medium,
    Low,
}

impl Priority {
    /// Linear's wire encoding.
    pub fn from_wire(value: u8) -> Self {
        match value {
            1 => Priority::Urgent,
            2 => Priority::High,
            3 => Priority::Medium,
            4 => Priority::Low,
            _ => Priority::None,
        }
    }

    /// Rank for display ordering: most urgent first, "no priority" last.
    ///
    /// This is the ordering the raw wire value does not give you.
    pub fn rank(&self) -> u8 {
        match self {
            Priority::Urgent => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::Low => 3,
            Priority::None => 4,
        }
    }
}

/// Whether a complete sweep still returns this record.
///
/// ADR-0007: absence demotes, never deletes. An id in the mirror and missing
/// from a *complete* sweep is marked [`Presence::Absent`] and drops out of
/// every working view, but stays in the snapshot forever — which is what
/// keeps "the mirror is the export" true as Linear auto-archives history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    Live,
    Absent {
        /// When the first complete sweep that missed this id ran.
        ///
        /// First, not latest: this is the answer to "how long has this been
        /// gone", so a later sweep that also misses it must not refresh the
        /// stamp. [`Presence::demote`] is what enforces that.
        since_ms: i64,
    },
}

impl Presence {
    /// Marks an absence, preserving the original `since_ms` if already absent.
    pub fn demote(&self, now_ms: i64) -> Presence {
        match self {
            Presence::Absent { since_ms } => Presence::Absent {
                since_ms: *since_ms,
            },
            Presence::Live => Presence::Absent { since_ms: now_ms },
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Presence::Live)
    }
}

/// One record in the mirror: an Item, which is an Action once past triage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    /// The human-facing key (`ION-16`). Display only — never a join key;
    /// [`Item::id`] is.
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub stage: Stage,
    /// The project's *name*, not its id.
    ///
    /// Denormalized deliberately. Nesting `issues` under `projects` in the
    /// sweep blows Linear's complexity ceiling (~57k against a 10k cap — see
    /// `.claude/skills/next-up-personal/REFERENCE.md`), so membership is
    /// derived from each issue's own `project { name }` on a flat query.
    /// [`super::Route`] is keyed by the same name.
    pub project: Option<String>,
    /// Label names, verbatim. Grouped labels (`size`, `energy`, context)
    /// arrive here undistinguished; typing them is ADR-0008's call, not this
    /// slice's, and guessing now would bake a mapping into the snapshot
    /// format before the decision exists.
    pub labels: Vec<String>,
    pub priority: Priority,
    /// `YYYY-MM-DD` verbatim from Linear (day-grained; Linear's own
    /// `dueDate` carries no time), or a minute-precision naive date-time
    /// once a record round-trips through the owned schema (ADR-0013). A
    /// calendar date is not an instant — parsing it to a timestamp requires
    /// a timezone this record has no business choosing.
    pub deadline: Option<String>,
    pub created_at_ms: i64,
    /// The base for ADR-0007's field-level conflict detection: a queued
    /// mutation records the value it saw here, and a sweep returning a newer
    /// one is what makes a conflict detectable at all.
    pub updated_at_ms: i64,
    /// Ids of the items blocking this one.
    ///
    /// From Linear's **`inverseRelations`** filtered to `type: "blocks"` —
    /// the inverse direction, confirmed live on ION-16. Forward `relations`
    /// is what this item blocks; getting it backwards inverts the frontier.
    /// Stored unfiltered by shut-ness because that is a *query-time*
    /// question: a blocker completed since the last sweep must stop blocking
    /// without re-fetching this item.
    pub blockers: Vec<ItemId>,
    pub url: Option<String>,
    pub presence: Presence,
    /// Fields Linear returned that this client does not model, verbatim.
    ///
    /// ADR-0001 rule 3: the mirror is the export. Without this, a partial
    /// mapping silently makes the export lossy, and the loss is invisible
    /// until someone needs the missing field back.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Item {
    /// A minimal live item, for tests and for the capture path (which mints
    /// exactly this and lets the sweep fill in the rest).
    pub fn new(id: impl Into<ItemId>, title: impl Into<String>, stage: Stage) -> Self {
        Self {
            id: id.into(),
            identifier: String::new(),
            title: title.into(),
            description: None,
            stage,
            project: None,
            labels: Vec::new(),
            priority: Priority::None,
            deadline: None,
            created_at_ms: 0,
            updated_at_ms: 0,
            blockers: Vec::new(),
            url: None,
            presence: Presence::Live,
            extra: serde_json::Map::new(),
        }
    }

    pub fn is_live(&self) -> bool {
        self.presence.is_live()
    }

    /// Live, and not finished — the population ADR-0001's 250-issue watchline
    /// counts.
    pub fn is_active(&self) -> bool {
        self.is_live() && !self.stage.is_shut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_and_grilling_are_not_actions() {
        // CONTEXT.md: an Action is startable without further planning.
        assert!(!Stage::Triage.is_action());
        assert!(!Stage::Grilling.is_action());
        assert!(Stage::Backlog.is_action());
        assert!(Stage::Ready.is_action());
        assert!(Stage::InProgress.is_action());
    }

    #[test]
    fn an_unmodelled_state_is_never_an_action() {
        // Unrecognized work must not reach the frontier by default.
        assert!(!Stage::Other("Waiting on Legal".to_string()).is_action());
    }

    #[test]
    fn canceled_counts_as_shut_alongside_done() {
        assert!(Stage::Done.is_shut());
        assert!(Stage::Canceled.is_shut());
        assert!(!Stage::Blocked.is_shut());
    }

    #[test]
    fn demotion_keeps_the_first_absence_timestamp() {
        // "How long has this been gone" is the question this answers, so a
        // second sweep that also misses the id must not reset the clock.
        let first = Presence::Live.demote(1_000);
        let second = first.demote(9_999);
        assert_eq!(second, Presence::Absent { since_ms: 1_000 });
    }

    #[test]
    fn priority_rank_orders_urgent_first_and_none_last() {
        // The wire encoding does neither: 0 is "no priority" and would sort
        // ahead of Urgent, and the rest are inverted.
        let mut wire = vec![0u8, 4, 1, 3, 2];
        wire.sort_by_key(|v| Priority::from_wire(*v).rank());
        let ranked: Vec<Priority> = wire.into_iter().map(Priority::from_wire).collect();
        assert_eq!(
            ranked,
            vec![
                Priority::Urgent,
                Priority::High,
                Priority::Medium,
                Priority::Low,
                Priority::None,
            ]
        );
    }

    #[test]
    fn an_unknown_wire_priority_degrades_to_none_rather_than_panicking() {
        assert_eq!(Priority::from_wire(9), Priority::None);
    }

    #[test]
    fn unmodelled_fields_survive_a_serde_round_trip() {
        // ADR-0001 rule 3: a partial mapping must not make the export lossy.
        let mut item = Item::new("id-1", "Ship it", Stage::Ready);
        item.extra
            .insert("estimate".to_string(), serde_json::Value::Number(3.into()));

        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();

        assert_eq!(back.extra.get("estimate").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(back, item);
    }

    #[test]
    fn an_empty_extra_bag_adds_nothing_to_the_serialized_form() {
        // The snapshot is meant to stay readable (ADR-0003) — an empty bag on
        // every record would be pure noise in a file people debug by reading.
        let item = Item::new("id-1", "Ship it", Stage::Ready);
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("extra"), "{json}");
    }

}
