//! THE single copy of the size/energy/context/frontier-axis vocabulary
//! (ADR-0025, #141/M1-2) — sunk here to kill the four-unlinked-copies bug
//! class the web used to carry: `field-vocabulary.ts`, `capture-meta.ts`,
//! `size-energy.ts` and `frontier-facets.ts` each held their own list of
//! the same size/energy words, tied together by nothing mechanical. That
//! was not hypothetical: ADR-0024 renamed the middle size (`short` ->
//! `normal`) and `field-vocabulary.ts`'s option list was the one copy that
//! did not get the memo — it kept compiling and kept writing a dead value,
//! because the server accepts `short` as a serde alias, and nothing but a
//! test would have caught it.
//!
//! **Size and Energy are reused, never re-derived.** `hummingbird_domain`
//! already owns the closed vocabularies themselves — [`Size::ALL`] and
//! [`Energy::ALL`] with their own `as_str` — so this module adds no third
//! spelling of "what the wire calls the middle size"; it only adds the
//! *display* pairing (a label) and the ordering a `<select>` renders in.
//!
//! **`CONTEXTS` is not a closed vocabulary.** CONTEXT.md: "an open
//! vocabulary, not a fixed enum … because the set of places a person works
//! is theirs." The list here is the *suggested* set every client's capture
//! form offers, which two clients still have to agree on even though
//! `items.context` itself accepts anything — see `field-vocabulary.ts`'s
//! header for the fuller argument and #500's PR description for why the
//! web's own `CONTEXTS` export stays a literal TS array rather than a live
//! call through the seam (a module-evaluation-order constraint, not a
//! decision that this list is exempt from being canonical here).

use hummingbird_domain::{Energy, Size};

/// One `<select>` option: a vocabulary's own wire value and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabOption {
    pub value: String,
    pub label: String,
}

/// `hummingbird_domain::Size`'s own three values, sentence-case labelled,
/// in [`Size::ALL`]'s order — the same order the capture sliders use (their
/// own `CAPTURE_SIZE_NAMES`/`CAPTURE_ENERGY_NAMES` stay a client-side
/// concern: a slider *stop index* is a rendering detail, never wire data).
pub fn size_options() -> Vec<VocabOption> {
    Size::ALL
        .iter()
        .map(|size| VocabOption { value: size.as_str().to_string(), label: capitalize(size.as_str()) })
        .collect()
}

pub fn energy_options() -> Vec<VocabOption> {
    Energy::ALL
        .iter()
        .map(|energy| VocabOption {
            value: energy.as_str().to_string(),
            label: capitalize(energy.as_str()),
        })
        .collect()
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The contexts the capture and edit forms *suggest* — the places this
/// system's owner actually works. Never a constraint on what
/// `items.context` may hold (see the module header): a context outside
/// this list is not an error, only unsuggested.
///
/// `@waiting` is deliberately absent: CONTEXT.md is flat that "External
/// wait is the only meaning of the Blocked state", so a context by that
/// name was the Blocked stage wearing a hard filter's clothes.
pub const CONTEXTS: [&str; 5] = ["@home", "@computer", "@phone", "@errands", "@garden"];

/// The frontier's facet/grouping axis names (M1-3, #501's `frontier-facets.ts`
/// `Facet` type) — defined here, ahead of M1-3, because #501 and #503 both
/// depend on this module existing (`to-goal` scope card, wave 2) and a
/// second, independently-typed axis list in `frontier-facets.ts` is exactly
/// the drift this module exists to prevent. M1-3 is the module's first web
/// consumer; nothing in M1-2 calls it.
pub const FRONTIER_AXES: [&str; 4] = ["context", "size", "energy", "urgency"];

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from `field-vocabulary.test.ts`'s size/energy describe block.
    // The retired TS suite pinned a *leading `""` "Not set" entry*; that
    // entry has no wire value, so it belongs to the TS form-adapter layer
    // (`capture-meta.ts`'s own "resting state" concern) and is prepended
    // there, not here — this module answers only "what are the real
    // values", which is what `.slice(1)` was already isolating on the TS
    // side of that same test.

    #[test]
    fn size_options_offers_exactly_the_wire_names_in_order() {
        let values: Vec<String> = size_options().into_iter().map(|o| o.value).collect();
        assert_eq!(values, ["quick", "normal", "deep"]);
    }

    #[test]
    fn energy_options_offers_exactly_the_wire_names_in_order() {
        let values: Vec<String> = energy_options().into_iter().map(|o| o.value).collect();
        assert_eq!(values, ["low", "medium", "high"]);
    }

    #[test]
    fn names_the_middle_size_normal_never_the_pre_adr_0024_short() {
        let values: Vec<String> = size_options().into_iter().map(|o| o.value).collect();
        assert!(values.iter().any(|v| v == "normal"));
        assert!(!values.iter().any(|v| v == "short"));
    }

    #[test]
    fn labels_are_sentence_case() {
        let labels: Vec<String> = size_options().into_iter().map(|o| o.label).collect();
        assert_eq!(labels, ["Quick", "Normal", "Deep"]);
    }

    #[test]
    fn suggests_the_places_this_systems_owner_works() {
        assert_eq!(CONTEXTS, ["@home", "@computer", "@phone", "@errands", "@garden"]);
    }

    #[test]
    fn does_not_suggest_waiting_which_was_the_blocked_stage_in_disguise() {
        assert!(!CONTEXTS.contains(&"@waiting"));
    }

    #[test]
    fn carries_no_resting_entry() {
        assert!(!CONTEXTS.contains(&""));
    }

    #[test]
    fn frontier_axes_are_the_four_facets_the_frontier_offers() {
        assert_eq!(FRONTIER_AXES, ["context", "size", "energy", "urgency"]);
    }
}
