//! Which calendars a device actually polls — sunk here from the web's
//! `effectiveSelection` by #564 because Android needs the identical rule,
//! and since #843 (ADR-0040 decision 8) the **only** copy: the web reaches
//! it through `ffi-web`'s `effective_calendar_selection_json` and its own
//! TS union is gone. The refusal that keeps the bound row from being
//! unticked moved into the core with it (`Core::toggle_calendar`'s
//! `CalendarPrefsError::Locked`).
//!
//! The decision itself is one sentence: the ticked calendars are the
//! operator's synced choice (`crate::calendar_prefs`), but the synced
//! `trips-calendar` binding is polled whatever the selection says, and
//! always at the long horizon — the Vacation pane asks about trips up to two
//! years out, and a calendar fetched at the standard window would answer it
//! with a confident, wrong "nothing booked". The union is derived at every
//! read, never written into the selection row, so re-binding re-computes
//! cleanly.

use super::{CalendarHorizon, CalendarSelection};

/// The ticked calendars ∪ the bound Trips calendar, each with its horizon.
///
/// Order is the stored order, with a bound calendar nobody ticked appended —
/// stable, so a re-push is byte-identical while nothing moved.
pub fn effective_selection(
    stored: &[CalendarSelection],
    trips_id: Option<&str>,
) -> Vec<CalendarSelection> {
    let mut selection: Vec<CalendarSelection> = stored
        .iter()
        .map(|entry| CalendarSelection {
            id: entry.id.clone(),
            horizon: if Some(entry.id.as_str()) == trips_id {
                CalendarHorizon::Long
            } else {
                entry.horizon
            },
        })
        .collect();
    if let Some(trips_id) = trips_id {
        if !stored.iter().any(|entry| entry.id == trips_id) {
            selection.push(CalendarSelection::long(trips_id));
        }
    }
    selection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(selection: &[CalendarSelection]) -> Vec<(&str, CalendarHorizon)> {
        selection.iter().map(|entry| (entry.id.as_str(), entry.horizon)).collect()
    }

    #[test]
    fn with_no_binding_the_stored_selection_is_the_polled_one() {
        let stored = vec![CalendarSelection::standard("a"), CalendarSelection::standard("b")];
        assert_eq!(
            ids(&effective_selection(&stored, None)),
            vec![("a", CalendarHorizon::Standard), ("b", CalendarHorizon::Standard)]
        );
    }

    #[test]
    fn a_bound_calendar_nobody_ticked_is_appended_at_the_long_horizon() {
        let stored = vec![CalendarSelection::standard("a")];
        assert_eq!(
            ids(&effective_selection(&stored, Some("trips@g"))),
            vec![("a", CalendarHorizon::Standard), ("trips@g", CalendarHorizon::Long)]
        );
    }

    #[test]
    fn a_bound_calendar_the_device_also_ticked_is_upgraded_in_place() {
        // Not appended twice, and not left at the standard window the
        // picker's own gesture gave it.
        let stored = vec![CalendarSelection::standard("trips@g"), CalendarSelection::standard("a")];
        assert_eq!(
            ids(&effective_selection(&stored, Some("trips@g"))),
            vec![("trips@g", CalendarHorizon::Long), ("a", CalendarHorizon::Standard)]
        );
    }
}
