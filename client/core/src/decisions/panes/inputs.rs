//! What a pane rule reads, and nothing else — the decisions-local input
//! shape for the whole `panes` family (#533/M4).
//!
//! Same discipline as [`super::super::frontier::FrontierItem`]: **do not
//! re-cross whole DTOs**. Every field here was added only once a sunk pane
//! actually read it — three at #533's waste-only probe, then the calendar
//! arm, the connected flag, the actionable items and the sync snapshot as
//! #534's seven arrived. A caller adding a field here is making a claim
//! that some rule reads it, and what crosses within a field is trimmed the
//! same way (see [`PaneItemFacts`], which is not an item DTO).
//!
//! Deliberately **not** [`crate::pane::PaneRead`]. That type is
//! `Serialize`-only and is a *read* type — what the core hands the host on
//! its way out of `Core::pane_read` — while this is the host's own input on
//! the way back in through a second, stateless wasm instance. Reusing it
//! would make one type answer to two directions and quietly require a
//! `Deserialize` derive on a view model.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::freshness::Freshness;

/// The clock, the bindings table and whatever pane reads have landed —
/// plus, for #534's seven new panes, the calendar arm, the actionable items
/// list, and the sync snapshot the reachability pane reads. Each of the
/// three was added only once a real sunk pane needed it (this module's own
/// discipline), never spun up ahead of a caller.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneInputs {
    pub now_ms: i64,
    /// `None` until the first `bindings` answer arrives — "nobody has read
    /// the table yet", which is **not** the same as "the table is empty".
    /// Reading the two as one showed every configured device the setup
    /// prompt for the whole round trip between mount and that answer.
    #[serde(default)]
    pub bindings: Option<Vec<BindingFact>>,
    /// Keyed by source, only what was actually requested. A missing entry
    /// is "not read yet", never "no rows".
    #[serde(default)]
    pub pane_reads: HashMap<String, PaneReadFacts>,
    /// Issue #267's calendar-reads arm (`QuestionInputs.calendarReads`),
    /// keyed by the caller's own request key, never a source. A missing
    /// entry is "not requested yet"; [`CalendarReadFacts::NotRead`] is the
    /// core's further distinction, "requested, but this device has never
    /// synced its calendar at all".
    #[serde(default)]
    pub calendar_reads: HashMap<String, CalendarReadFacts>,
    /// Whether this device has ever connected a calendar at all —
    /// `QuestionInputs.calendarConnected`'s own field, distinct from
    /// whether any one calendar-arm *read* has landed.
    #[serde(default)]
    pub calendar_connected: bool,
    /// Every actionable item this device currently knows about
    /// (`QuestionInputs.items`), trimmed to the fields a sunk pane reads.
    #[serde(default)]
    pub items: Vec<PaneItemFacts>,
    /// The device's latest authority-sync facts
    /// (`QuestionInputs.sync`/`QuestionSyncSnapshot`) — read only by the
    /// reachability pane, which has no `context_snapshots` lane of its own.
    #[serde(default)]
    pub sync: SyncFacts,
}

impl PaneInputs {
    /// The binding row for `key`, if the table has been read and holds one.
    pub fn binding(&self, key: &str) -> Option<&BindingFact> {
        self.bindings.as_ref()?.iter().find(|binding| binding.key == key)
    }

    /// One source's snapshot row under `key`, if that source has been read
    /// at all.
    pub fn snapshot(&self, source: &str, key: &str) -> Option<&PaneSnapshotFacts> {
        self.pane_reads.get(source)?.snapshots.iter().find(|snapshot| snapshot.key == key)
    }

    /// One source's live alerts, or an empty slice when that source has not
    /// been read at all — never a distinct "not read" state, since no sunk
    /// pane needs one for alerts (`race.rs`'s own join reads this).
    pub fn live_alerts(&self, source: &str) -> &[PaneAlertFacts] {
        self.pane_reads.get(source).map(|read| read.live_alerts.as_slice()).unwrap_or(&[])
    }
}

/// One `settings` row as a pane rule reads it — the key and its value, with
/// `known`/`pending` (the editor's concerns) left behind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingFact {
    pub key: String,
    pub value: BindingValueFact,
}

/// Three states rather than `Option<String>`: "no row" and "a row holding
/// something that is not text" are different facts, and collapsing the
/// second into the first would let a pane claim the reader never set
/// something they can see they did. `Other`'s stored JSON is deliberately
/// not carried — displaying it is the editor's job, and no *decision* reads
/// it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum BindingValueFact {
    Unset,
    Text { text: String },
    Other,
}

/// One source's pane read, as a rule reads it: its snapshot rows, plus
/// (#534) its live alerts — added because the race pane joins its
/// race-start alert onto a series pane by `subjectKey`, the one field this
/// crossing carries.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneReadFacts {
    #[serde(default)]
    pub snapshots: Vec<PaneSnapshotFacts>,
    #[serde(default)]
    pub live_alerts: Vec<PaneAlertFacts>,
}

/// One live alert, trimmed to the one field a sunk pane's join reads
/// (`PaneAlertDTO`'s `subjectKey`) — `race.rs`'s join is the only reader.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneAlertFacts {
    pub subject_key: Option<String>,
}

/// One `items` row, trimmed to the fields a sunk pane's item reasoning
/// reads — never the whole DTO, on this module's own discipline.
///
/// Four fields at #534 (the weekend pane's merge: `id`/`title`/`deadline`/
/// `scheduledDate`), eight since #675, which added the first pane whose
/// *subject* is the operator's own items rather than an outside source.
/// Each of the four newcomers was added because [`super::homework`] reads
/// it and for no other reason: `stage` classifies "open" (and pins the
/// weekend merge against the widened list — see
/// [`super::weekend`]'s merge), `context` is the match, `description` is
/// the expanded body, and `created_at` is the deadline-less tiebreak.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneItemFacts {
    pub id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
    /// `TaskItemDTO`'s `stage`, as the wire spells it (`triage`,
    /// `grilling`, `ready`, `in_progress`, `blocked`, `done`) — a plain
    /// string rather than a second closed enum here, on
    /// [`SyncFacts::latest_outcome_kind`]'s own reasoning: this crate has
    /// no reason to reject a stage it does not recognise, only to classify
    /// the ones a rule names.
    pub stage: String,
    /// The item's context (`@home`, `@homework`, …), or `None`. Matched
    /// case-insensitively after a trim by [`super::homework`]; never
    /// registry-checked, since `items.context` is an open vocabulary
    /// (`decisions::vocabulary`'s own header).
    pub context: Option<String>,
    /// The item's notes. The one field here that no *band* reads: it
    /// crosses because [`super::homework::homework_facts`] is what both
    /// clients' expanded bodies render, and a pane that made each client
    /// re-fetch the winning item's notes for itself would be two lookups
    /// that can disagree about which item won.
    pub description: Option<String>,
    /// Epoch ms, `TaskItemDTO`'s own `createdAt` — read only as
    /// [`super::homework`]'s tiebreak among deadline-less items.
    pub created_at: i64,
}

/// One calendar event, trimmed to the fields a sunk pane's calendar arm
/// reads (`CalendarEventDTO`) — never `recurrenceId`/`organizer`/
/// `providerUpdatedAtMs`/`htmlLink`, which no sunk pane reads.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventFacts {
    pub provider_event_id: String,
    pub calendar_id: String,
    pub title: String,
    pub when: CalendarEventWhenFacts,
    pub location: Option<String>,
    pub status: CalendarEventStatusFact,
}

/// `CalendarEventWhenDTO` verbatim — an all-day arm carries civil dates and
/// no instant, a timed arm carries instants and no zone (ADR-0015's
/// 2026-08-10 amendment); this crossing keeps that split rather than
/// flattening it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CalendarEventWhenFacts {
    AllDay { start_date: String, end_date: String },
    Timed { start_ms: i64, end_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CalendarEventStatusFact {
    Confirmed,
    Tentative,
    Cancelled,
}

/// `CalendarReadDTO` verbatim — the three-state "a gap is not an absence"
/// read `AnswerState` documents, applied to one calendar request.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CalendarReadFacts {
    NotRead,
    Read { events: Vec<CalendarEventFacts>, freshness: FreshnessFact },
}

/// `QuestionSyncSnapshot` verbatim, trimmed to what the reachability pane
/// reads. `latest_outcome_kind` carries `TaskRunOutcomeKind`'s own wire
/// spelling as a plain string rather than a second closed enum here: this
/// crate has no reason to reject a kind it does not recognise, only to
/// classify the ones [`super::reachability::sync_outcome_class`] knows —
/// exactly `Core::bindings`' reading of an unrecognised binding key.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFacts {
    pub latest_outcome_kind: Option<String>,
    pub latest_informative_at_ms: Option<i64>,
    pub last_successful_at_ms: Option<i64>,
}

/// One `context_snapshots` row as a rule reads it. `fetchedAtMs` is absent
/// on purpose: the age was already measured core-side
/// ([`Freshness::measure`]'s clock rule) and re-deriving it here would be
/// the second place that subtraction happens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSnapshotFacts {
    pub key: String,
    pub envelope: PaneEnvelopeFacts,
    pub freshness: FreshnessFact,
}

/// One snapshot row's envelope, read shallowly. `schema` rides through
/// untouched and is **never** registry-checked (ADR-0015): a source this
/// build has not heard of is a fact about the build, and the pane says so
/// itself.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PaneEnvelopeFacts {
    Ok { schema: String, body: String },
    Malformed { reason: String },
}

/// [`Freshness`] in the host's own wire spelling (`FreshnessDTO`,
/// `store/protocol.ts`), which is camelCase and tagged `kind` where
/// `Freshness`'s own serde is snake_case and tagged `state`.
///
/// A second spelling rather than a re-tagged `Freshness`, because the
/// existing wire is what the worker already emits and this seam is a
/// *second* reader of it — changing `Freshness`'s tags to suit this
/// crossing would move the existing one. The *reading* is not duplicated:
/// [`FreshnessFact::is_stale_beyond`] converts and defers to
/// [`Freshness::is_stale_beyond`], so the "`unknown` is never fresh" rule
/// is still spelled exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FreshnessFact {
    Unknown,
    Age { age_ms: i64, declared_cadence_ms: Option<i64> },
}

impl FreshnessFact {
    pub fn to_freshness(self) -> Freshness {
        match self {
            FreshnessFact::Unknown => Freshness::Unknown,
            FreshnessFact::Age { age_ms, declared_cadence_ms } => {
                Freshness::Age { age_ms, declared_cadence_ms }
            }
        }
    }

    /// Whether this answer is old enough to say so, against the caller's
    /// own threshold — [`Freshness::is_stale_beyond`] verbatim, so no pane
    /// can lose the `Unknown` arm by writing `age_ms > threshold` alone.
    pub fn is_stale_beyond(self, threshold_ms: i64) -> bool {
        self.to_freshness().is_stale_beyond(threshold_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUTS: &str = r#"{
        "nowMs": 1000,
        "bindings": [
            {"key":"city-waste-page","known":true,"pending":false,
             "value":{"state":"text","text":"https://example.gov"}}
        ],
        "paneReads": {
            "city-waste/v2": {
                "source": "city-waste/v2",
                "snapshots": [
                    {"key":"collection","fetchedAtMs":900,
                     "envelope":{"kind":"ok","schema":"city-waste/v2","polledEveryMs":86400000,"body":"{}"},
                     "freshness":{"kind":"age","ageMs":100,"declaredCadenceMs":86400000}}
                ],
                "liveAlerts": []
            }
        },
        "calendarReads": {},
        "calendarConnected": false,
        "items": []
    }"#;

    #[test]
    fn reads_the_three_fields_a_rule_needs_out_of_the_hosts_whole_question_inputs() {
        // The literal above is `QuestionInputs`-shaped, extra fields and
        // all — the point is that the crossing ignores everything no rule
        // reads rather than forcing the host to build a second object.
        let inputs: PaneInputs = serde_json::from_str(INPUTS).unwrap();
        assert_eq!(inputs.now_ms, 1000);
        assert!(matches!(
            inputs.binding("city-waste-page").map(|b| &b.value),
            Some(BindingValueFact::Text { text }) if text == "https://example.gov"
        ));
        assert!(inputs.snapshot("city-waste/v2", "collection").is_some());
        assert!(inputs.snapshot("city-waste/v2", "nope").is_none());
        assert!(inputs.snapshot("nope", "collection").is_none());
    }

    #[test]
    fn an_absent_bindings_table_is_not_an_empty_one() {
        let unread: PaneInputs = serde_json::from_str(r#"{"nowMs":0,"bindings":null}"#).unwrap();
        assert!(unread.bindings.is_none());
        let empty: PaneInputs = serde_json::from_str(r#"{"nowMs":0,"bindings":[]}"#).unwrap();
        assert_eq!(empty.bindings.as_deref(), Some(&[][..]));
        assert!(unread.binding("anything").is_none());
        assert!(empty.binding("anything").is_none());
    }

    #[test]
    fn a_binding_holding_something_that_is_not_text_is_its_own_state() {
        let inputs: PaneInputs = serde_json::from_str(
            r#"{"nowMs":0,"bindings":[{"key":"k","value":{"state":"other","raw":"7"}}]}"#,
        )
        .unwrap();
        assert_eq!(inputs.binding("k").map(|b| b.value.clone()), Some(BindingValueFact::Other));
    }

    #[test]
    fn freshness_crosses_in_the_hosts_spelling_and_defers_to_the_core_stale_rule() {
        let unknown: FreshnessFact = serde_json::from_str(r#"{"kind":"unknown"}"#).unwrap();
        assert_eq!(unknown.to_freshness(), Freshness::Unknown);
        // Never fresh, against any threshold — the whole reason this is a
        // tagged union rather than a nullable age.
        assert!(unknown.is_stale_beyond(i64::MAX));

        let aged: FreshnessFact =
            serde_json::from_str(r#"{"kind":"age","ageMs":100,"declaredCadenceMs":null}"#).unwrap();
        assert!(!aged.is_stale_beyond(100));
        assert!(aged.is_stale_beyond(99));
    }
}
