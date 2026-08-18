//! #274's backend picker: the device-local selection's degrade-to-Auto rule
//! and the tier fallback, sunk here from `client/web/src/skills/
//! backend-registry.ts`'s `fallbackEntry` and `backend-selection.ts`'s
//! `readBackendSelection` at #539.
//!
//! **The registry's data — label, model, endpoint, connect timeout — stays
//! per-client.** It is configuration, not a decision two clients could
//! disagree about being *wrong*: `BACKEND_REGISTRY`/Android's own registry
//! list may render a different label for the same id without that being a
//! bug. What IS a decision, and so lives here: which id a picker degrades a
//! stale selection to, and which id a one-tap fallback offers after a pin
//! declines. Both take the registry as a bare ordered list of ids — the only
//! part of an entry either rule reads.

use super::decline::NO_TOKEN;
use super::run::SkillRunState;

/// The sentinel selection value. Not a registered id — no entry may ever be
/// named `"auto"`, which is why a picker checks this constant rather than
/// `registry.is_empty()`.
pub const AUTO_SELECTION: &str = "auto";

/// The one-tap fallback offered when a pin is declined: the next registered
/// id that is not the dead one, in registry order. `None` when there is
/// none.
pub fn fallback_backend_id(registry_ids: &[String], dead_id: &str) -> Option<String> {
    registry_ids.iter().find(|id| id.as_str() != dead_id).cloned()
}

/// #274's one-tap fallback offer, decided whole rather than assembled from
/// three separate reads by each caller — the same decision
/// `useMicrotaskWiring.ts`'s `declinedFallback` makes on the web, sunk here
/// at #539's review:
///
/// - not declined at all → `None`;
/// - the current selection is Auto (nothing to fall back FROM, Auto already
///   tried everything it could) → `None`;
/// - the decline evidences no reachability problem — no token was ever sent
///   ([`NO_TOKEN`]'s own words) or a backend *answered* (`answered: true`,
///   so the guard that declined it is the seam's, not any backend's) →
///   `None`;
/// - otherwise, [`fallback_backend_id`]'s answer over `registry_ids`, which
///   is itself `None` when there is nowhere else to try.
pub fn declined_backend_fallback(
    state: &SkillRunState,
    selection: &str,
    registry_ids: &[String],
) -> Option<String> {
    let SkillRunState::Declined { reason, answered, .. } = state else {
        return None;
    };
    if selection == AUTO_SELECTION {
        return None;
    }
    if *answered {
        return None;
    }
    if reason == NO_TOKEN {
        return None;
    }
    fallback_backend_id(registry_ids, selection)
}

/// Auto when nothing is stored, and Auto when the stored value no longer
/// names a registered entry — the same "a stale binding degrades to the
/// safe default" rule `route-plan.ts`'s `planRoute` applies at routing time,
/// kept here too so a picker never renders a selection it cannot label.
pub fn resolve_backend_selection(stored: Option<&str>, registry_ids: &[String]) -> String {
    match stored {
        None => AUTO_SELECTION.to_string(),
        Some(raw) if raw == AUTO_SELECTION => AUTO_SELECTION.to_string(),
        Some(raw) if registry_ids.iter().any(|id| id == raw) => raw.to_string(),
        Some(_) => AUTO_SELECTION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// Cases ported from `backend-registry.test.ts`'s `fallbackEntry` suite.
    #[test]
    fn fallback_backend_id_finds_the_next_entry_that_is_not_the_dead_one() {
        assert_eq!(fallback_backend_id(&ids(&["a", "b"]), "a").as_deref(), Some("b"));
    }

    #[test]
    fn fallback_backend_id_is_none_when_the_dead_entry_is_the_only_one() {
        assert_eq!(fallback_backend_id(&ids(&["cloud"]), "cloud"), None);
    }

    /// Cases ported from `backend-selection.test.ts`.
    #[test]
    fn defaults_to_auto_when_nothing_is_stored() {
        assert_eq!(resolve_backend_selection(None, &ids(&["cloud"])), AUTO_SELECTION);
    }

    #[test]
    fn round_trips_a_pinned_selection() {
        assert_eq!(resolve_backend_selection(Some("cloud"), &ids(&["cloud"])), "cloud");
    }

    #[test]
    fn round_trips_auto_explicitly_stored() {
        assert_eq!(resolve_backend_selection(Some(AUTO_SELECTION), &ids(&["cloud"])), AUTO_SELECTION);
    }

    #[test]
    fn a_stored_selection_naming_a_retired_backend_degrades_to_auto() {
        assert_eq!(resolve_backend_selection(Some("retired-backend"), &ids(&["cloud"])), AUTO_SELECTION);
    }

    /// Cases moved here from `ffi-mobile`'s `skills_tests` at #539's review
    /// (round 2): the predicate itself belongs in the core, not the seam.
    fn declined(reason: &str, answered: bool) -> SkillRunState {
        SkillRunState::Declined {
            messages: Vec::new(),
            reason: reason.to_string(),
            backend: None,
            model: None,
            answered,
        }
    }

    #[test]
    fn declined_backend_fallback_offers_the_next_id_only_for_an_unanswered_non_no_token_decline() {
        assert_eq!(
            declined_backend_fallback(
                &declined("Could not reach the server.", false),
                "cloud",
                &ids(&["cloud", "home"]),
            ),
            Some("home".to_string()),
        );
    }

    #[test]
    fn declined_backend_fallback_is_none_when_not_declined() {
        assert_eq!(declined_backend_fallback(&SkillRunState::Idle, "cloud", &ids(&["cloud"])), None);
        assert_eq!(
            declined_backend_fallback(
                &SkillRunState::Running { messages: Vec::new() },
                "cloud",
                &ids(&["cloud"]),
            ),
            None,
        );
    }

    #[test]
    fn declined_backend_fallback_is_none_when_the_selection_is_auto() {
        assert_eq!(
            declined_backend_fallback(
                &declined("Could not reach the server.", false),
                AUTO_SELECTION,
                &ids(&["cloud", "home"]),
            ),
            None,
        );
    }

    #[test]
    fn declined_backend_fallback_is_none_for_a_decline_a_backend_answered() {
        assert_eq!(
            declined_backend_fallback(
                &declined("That item already has live steps.", true),
                "cloud",
                &ids(&["cloud", "home"]),
            ),
            None,
        );
    }

    #[test]
    fn declined_backend_fallback_is_none_for_no_token_even_though_unanswered() {
        assert_eq!(
            declined_backend_fallback(&declined(NO_TOKEN, false), "cloud", &ids(&["cloud", "home"])),
            None,
        );
    }

    #[test]
    fn declined_backend_fallback_is_none_when_there_is_nowhere_else_to_try() {
        assert_eq!(
            declined_backend_fallback(&declined("Could not reach the server.", false), "cloud", &ids(&["cloud"])),
            None,
        );
    }
}
