//! **Calendar choices** (ADR-0040 decision 8): whether the operator's
//! devices poll Google Calendar at all, and which calendars at which
//! horizon — two `settings` rows every device reads, where each device used
//! to keep its own copy (the web in `localStorage`, the phone in a
//! DataStore). The fourth typed vocabulary over the `settings` table, beside
//! [`crate::bindings`], [`crate::question_switch`] and [`crate::contexts`],
//! on ADR-0034/0038's recipe: frozen key spellings, no row or an unreadable
//! row reads as the safe default, subtracted from [`crate::Core::bindings`],
//! written through the one setting-write path and overlaid so an offline
//! edit reads back at once.
//!
//! # Why two rows
//!
//! [`CALENDAR_CONNECTED_KEY`] is a JSON boolean, and
//! [`CALENDAR_SELECTION_KEY`] is the ticked calendars, each with its
//! horizon, as one versioned object. Two rows so a Connect on one device and
//! a picker edit on another never race one CAS; one row for the selection
//! because one picker edits it as a set (ADR-0038 decision 3's argument for
//! one ordered row, not ADR-0034's per-entry rows).
//!
//! # Connecting is a preference, not a consent
//!
//! Since ADR-0028 no device holds a Google grant of its own — every device
//! mints from the authority's one server-held credential — so "connected"
//! says only that the operator wants calendar context on their devices.
//! Syncing it is what lets a newly signed-in device start polling without a
//! Connect tap.
//!
//! # Every edit goes through the core
//!
//! [`toggled`] keeps every other entry's horizon, so a web toggle — the web
//! has no horizon control — never drops a long horizon only the phone can
//! set. The bound Trips calendar is still added at read time by
//! [`crate::calendar::effective_selection`], never written into this row
//! (`calendar/selection.rs`'s header has why).
//!
//! # Migration from the device-local copies
//!
//! [`crate::Core::seed_calendar_prefs`] is the one-time door a host hands
//! its old local values to. It waits for this device's first completed full
//! sweep — before that, "no row" may mean "not pulled yet" — and writes only
//! a row that does not exist. Two devices seeding at once both send a
//! create-shaped write (`expected_version: 0`); the authority answers the
//! second with the first's row (`handlers/settings.rs`' create-replay
//! rule). An ordinary edit treats that divergent replay as a conflict and
//! dead-letters it; a seed defends no field (`lib.rs`'s
//! `SettingWrite::CreateIfAbsent`), so the loser adopts the winner with no
//! 409 and no dead letter.
//!
//! # The spellings are frozen
//!
//! `settings` has no DELETE, so respelling either key would orphan every
//! device's choices. `the_keys_are_frozen` is the pin.

use serde::{Deserialize, Serialize};

use crate::calendar::google::{CalendarHorizon, CalendarSelection};

/// Whether the operator's devices poll Google Calendar — a JSON boolean.
pub const CALENDAR_CONNECTED_KEY: &str = "calendar-connected";

/// Which calendars are polled, at which horizon —
/// `{"v":1,"calendars":[{"id":…,"horizon":"standard"|"long"}]}`.
pub const CALENDAR_SELECTION_KEY: &str = "calendar-selection";

/// Both keys, for [`crate::Core::bindings`]' subtraction.
pub const CALENDAR_PREF_KEYS: [&str; 2] = [CALENDAR_CONNECTED_KEY, CALENDAR_SELECTION_KEY];

/// The selection row's shape version. A row carrying any other `v` was
/// written by a build this one cannot read, and reads as empty.
const SELECTION_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct StoredSelection {
    v: u32,
    calendars: Vec<CalendarSelection>,
}

/// Reads one stored `calendar-connected` value. Only JSON `true` is
/// connected; anything else — `false`, a string, unparseable bytes — is not,
/// so an unreadable row never starts a device polling.
pub fn connected_from_stored(raw: &str) -> bool {
    matches!(serde_json::from_str::<serde_json::Value>(raw), Ok(serde_json::Value::Bool(true)))
}

/// Reads one stored `calendar-selection` value. Anything but a version-1
/// object whose every entry has a string id and a known horizon reads as
/// **empty** — the safe default, which polls nothing rather than guessing.
///
/// Sanitised on the way in, as [`crate::contexts::contexts_from_stored`]
/// is: a blank id is dropped, and a later entry repeating an earlier id is
/// dropped too. A row this build did not write must never put a duplicate
/// row in the picker or poll one calendar twice.
pub fn selection_from_stored(raw: &str) -> Vec<CalendarSelection> {
    let Ok(stored) = serde_json::from_str::<StoredSelection>(raw) else {
        return Vec::new();
    };
    if stored.v != SELECTION_VERSION {
        return Vec::new();
    }
    sanitised(stored.calendars)
}

/// Trims every id, then drops a blank one and any later entry repeating an
/// earlier id — [`selection_from_stored`]'s rule, and the one a host's old
/// local values pass through before they are seeded.
pub fn sanitised(entries: Vec<CalendarSelection>) -> Vec<CalendarSelection> {
    let mut selection: Vec<CalendarSelection> = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = entry.id.trim();
        if id.is_empty() || selection.iter().any(|existing| existing.id == id) {
            continue;
        }
        selection.push(CalendarSelection {
            id: id.to_string(),
            horizon: entry.horizon,
        });
    }
    selection
}

/// The wire value a selection is written as — [`selection_from_stored`]'s
/// inverse.
pub fn selection_to_wire(selection: &[CalendarSelection]) -> serde_json::Value {
    serde_json::to_value(StoredSelection {
        v: SELECTION_VERSION,
        calendars: selection.to_vec(),
    })
    .expect("a calendar selection always serializes")
}

/// `selection` with `id` ticked or unticked. A newly ticked calendar is
/// appended at the standard horizon; every other entry keeps its own
/// horizon and its place.
pub fn toggled(selection: &[CalendarSelection], id: &str) -> Vec<CalendarSelection> {
    if selection.iter().any(|entry| entry.id == id) {
        selection.iter().filter(|entry| entry.id != id).cloned().collect()
    } else {
        let mut next = selection.to_vec();
        next.push(CalendarSelection::standard(id));
        next
    }
}

/// `selection` with `id`'s horizon set, or `None` when `id` is not ticked —
/// a horizon belongs to a ticked calendar, and setting one must not tick it.
pub fn with_horizon(
    selection: &[CalendarSelection],
    id: &str,
    horizon: CalendarHorizon,
) -> Option<Vec<CalendarSelection>> {
    if !selection.iter().any(|entry| entry.id == id) {
        return None;
    }
    Some(
        selection
            .iter()
            .map(|entry| {
                if entry.id == id {
                    CalendarSelection { id: entry.id.clone(), horizon }
                } else {
                    entry.clone()
                }
            })
            .collect(),
    )
}

/// The calendar choices as a reader sees them. `pending` is one fact for
/// both rows: whether an unconfirmed local write is overlaid on either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarPrefs {
    pub connected: bool,
    /// The ticked calendars, in the row's order — never the polled set,
    /// which adds the bound Trips calendar
    /// ([`crate::Core::effective_calendar_selection`]).
    pub selection: Vec<CalendarSelection>,
    pub pending: bool,
}

/// What [`crate::Core::seed_calendar_prefs`] did with a host's old local
/// values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSeed {
    /// This device has not completed a full sweep yet, so "no row" cannot
    /// be told from "not pulled yet". Nothing was written; the host keeps
    /// its local values and asks again after a later cycle.
    NotYet,
    /// At least one row was written from the local values.
    Seeded,
    /// Nothing needed writing: every row the local values would have seeded
    /// already exists (another device's, which wins), or the local values
    /// were the defaults.
    Adopted,
}

impl CalendarSeed {
    /// Whether the host may now delete its local values.
    pub fn settled(self) -> bool {
        !matches!(self, CalendarSeed::NotYet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_frozen() {
        assert_eq!(CALENDAR_CONNECTED_KEY, "calendar-connected");
        assert_eq!(CALENDAR_SELECTION_KEY, "calendar-selection");
    }

    #[test]
    fn the_keys_collide_with_no_other_vocabulary() {
        for key in CALENDAR_PREF_KEYS {
            assert!(crate::bindings::BindingKey::parse(key).is_none(), "{key}");
            assert!(!crate::question_switch::all_switch_keys().contains(key), "{key}");
            assert_ne!(key, crate::contexts::CONTEXTS_KEY);
        }
    }

    #[test]
    fn only_json_true_is_connected() {
        assert!(connected_from_stored("true"));
        for raw in ["false", "\"true\"", "1", "null", "{}", "", "not json"] {
            assert!(!connected_from_stored(raw), "{raw}");
        }
    }

    #[test]
    fn a_version_one_selection_reads_back_with_its_horizons() {
        assert_eq!(
            selection_from_stored(
                r#"{"v":1,"calendars":[{"id":"a","horizon":"standard"},{"id":"trips","horizon":"long"}]}"#
            ),
            vec![CalendarSelection::standard("a"), CalendarSelection::long("trips")]
        );
    }

    #[test]
    fn an_unreadable_selection_is_empty() {
        for raw in [
            "",
            "not json",
            "null",
            "[]",
            r#"["a"]"#,
            r#"{"calendars":[{"id":"a","horizon":"standard"}]}"#,
            r#"{"v":2,"calendars":[{"id":"a","horizon":"standard"}]}"#,
            r#"{"v":1,"calendars":[{"id":"a","horizon":"medium"}]}"#,
            r#"{"v":1,"calendars":[{"id":7,"horizon":"standard"}]}"#,
            r#"{"v":1,"calendars":"a"}"#,
        ] {
            assert_eq!(selection_from_stored(raw), Vec::new(), "{raw}");
        }
    }

    #[test]
    fn a_selection_is_sanitised_on_the_way_in() {
        assert_eq!(
            selection_from_stored(
                r#"{"v":1,"calendars":[{"id":" a ","horizon":"long"},{"id":"","horizon":"standard"},{"id":"a","horizon":"standard"},{"id":"b","horizon":"standard"}]}"#
            ),
            vec![CalendarSelection::long("a"), CalendarSelection::standard("b")]
        );
    }

    #[test]
    fn the_wire_value_is_the_brief_s_shape_and_round_trips() {
        let selection = vec![CalendarSelection::standard("a"), CalendarSelection::long("trips")];
        let wire = selection_to_wire(&selection);
        // Compared as JSON, not text: a `Value` serializes its keys sorted,
        // which is also the canonical text the authority stores.
        assert_eq!(
            wire,
            serde_json::from_str::<serde_json::Value>(
                r#"{"v":1,"calendars":[{"id":"a","horizon":"standard"},{"id":"trips","horizon":"long"}]}"#
            )
            .unwrap()
        );
        assert_eq!(selection_from_stored(&wire.to_string()), selection);
    }

    #[test]
    fn a_toggle_keeps_every_other_entry_s_horizon_and_place() {
        let selection = vec![CalendarSelection::long("trips"), CalendarSelection::standard("a")];
        assert_eq!(
            toggled(&selection, "b"),
            vec![
                CalendarSelection::long("trips"),
                CalendarSelection::standard("a"),
                CalendarSelection::standard("b")
            ]
        );
        assert_eq!(toggled(&selection, "a"), vec![CalendarSelection::long("trips")]);
    }

    #[test]
    fn a_horizon_is_set_only_on_a_ticked_calendar() {
        let selection = vec![CalendarSelection::standard("a"), CalendarSelection::standard("b")];
        assert_eq!(
            with_horizon(&selection, "b", CalendarHorizon::Long),
            Some(vec![CalendarSelection::standard("a"), CalendarSelection::long("b")])
        );
        assert_eq!(with_horizon(&selection, "c", CalendarHorizon::Long), None);
    }

    #[test]
    fn the_read_crosses_with_snake_case_horizons() {
        assert_eq!(
            serde_json::to_string(&CalendarPrefs {
                connected: true,
                selection: vec![CalendarSelection::long("a")],
                pending: false,
            })
            .unwrap(),
            r#"{"connected":true,"selection":[{"id":"a","horizon":"long"}],"pending":false}"#
        );
        assert_eq!(serde_json::to_string(&CalendarSeed::NotYet).unwrap(), r#""not_yet""#);
    }
}
