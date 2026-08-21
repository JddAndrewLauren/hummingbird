//! Which calendars a device actually polls — `client/web/src/calendar/
//! selection.ts`'s `effectiveSelection`, sunk here by #564 because Android
//! needs the identical rule and a second hand-written union is exactly the
//! drift ADR-0025 exists to prevent. `acceptSelectionChange` (the refusal
//! that keeps the bound row from being unticked) stays TS for now: it is a
//! picker affordance, and Android has no locked row to refuse yet.
//!
//! **The web is not rewired to this yet** (its copy still runs, unchanged,
//! behind `useCalendarWiring.ts`): reaching these from a browser needs an
//! `ffi-web` export that #564 did not scope. ADR-0025's row records the
//! divergence rather than claiming a sink that only one client can see.
//!
//! The decision itself is one sentence: the *device's* ticked calendars are
//! its own business, but the synced `trips-calendar` binding is polled
//! whatever the device thinks, and always at the long horizon — the Vacation
//! pane asks about trips up to two years out, and a calendar fetched at the
//! standard window would answer it with a confident, wrong "nothing booked".

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
