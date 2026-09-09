//! The **suggested-contexts list** (ADR-0038): one `settings` row holding
//! the ordered contexts every capture and edit form offers and the
//! frontier's context chips order by. The third typed vocabulary over the
//! `settings` table, beside [`crate::bindings`] (text facts a pane reads)
//! and [`crate::question_switch`] (one boolean per question).
//!
//! `items.context` stays free text — CONTEXT.md's **Context**: "an open
//! vocabulary, not a fixed enum". This list constrains nothing; it is what
//! the forms *suggest*, and until the operator edits it in Settings it is
//! the build's own [`DEFAULT_CONTEXTS`].
//!
//! # Why this is not a [`crate::bindings::BindingKey`]
//!
//! ADR-0034 decision 2's argument, again: a binding's value is text in a
//! free-text box, and this is an ordered list with its own editor — chips,
//! an add field, a removal that cascades. Routed through the bindings
//! editor it would be a JSON array in a text field, and
//! [`crate::Core::bindings`] subtracts this row for the same reason it
//! subtracts the switches.
//!
//! # Why one row, not one row per context
//!
//! The list's **order is meaningful**: [`crate::decisions::frontier::contexts_of`]
//! orders the frontier's chips by it, suggested first in list order. Per-name
//! rows would carry no order, and — `settings` having no DELETE — would
//! accrete one permanent row per context ever tried. One row keeps the
//! order and keeps the table bounded.
//!
//! The cost is concurrency: two devices editing the list at the same
//! moment are two CAS writes to one version of one row, so the second
//! 409s on the same field, dead-letters, and its overlay reverts — visibly,
//! in the dead-letter journal, never silently. An operator editing their
//! own context list from two devices in the same second is the case this
//! accepts, on the same reasoning [`crate::question_switch`]'s header gives
//! for the opposite choice: there the rows *had* no order to lose.
//!
//! # Removal cascades; archived items are never touched
//!
//! Removing a context clears it from every live item carrying it and from
//! every project whose `default_context` names it ([`crate::Core::remove_context`]),
//! each as its own ordinary CAS write, so a conflict dead-letters one
//! named item rather than the batch. Archived items are not in
//! `Core::overlaid_items` at all — **Recall**'s rule, history stays
//! readable and never editable — so they keep whatever context they had.
//!
//! # The matching rule
//!
//! "Carries this context" is [`crate::rank::normalize_context`]'s reading —
//! trimmed, leading `@` dropped, ASCII-lowercased — so `@Errands` and
//! `errands` go when `@errands` is removed, exactly as the ranker's hard
//! filter already treats them as one context. Duplicate detection on add
//! uses the same rule. The homework pane's own tolerant match
//! (`panes::homework`) is that pane's affordance and unaffected.
//!
//! # The spelling is frozen
//!
//! `settings` has no DELETE, so respelling [`CONTEXTS_KEY`] would orphan
//! the operator's list and silently restore the defaults.
//! `the_key_is_frozen` is the pin.

use serde::Serialize;

use crate::decisions::frontier::NO_CONTEXT;
pub use crate::decisions::vocabulary::DEFAULT_CONTEXTS;
use crate::rank::normalize_context;

/// The `settings.key` the list is stored under — frozen (see the module
/// header).
pub const CONTEXTS_KEY: &str = "contexts";

/// Reads one stored `settings.value` as the list. `Some` only for a JSON
/// array of strings; anything else — an object, a number, unparseable
/// bytes, a value a newer build wrote — is `None`, and the caller falls
/// back to [`DEFAULT_CONTEXTS`]: a value this build cannot interpret must
/// not empty every capture form's suggestions.
///
/// **Every entry is sanitised on the way in**, by the same rules
/// [`crate::Core::add_context`] applies on the way out: trimmed, blank and
/// reserved names dropped, and a later entry naming the same context as an
/// earlier one (under [`same_context`]) dropped too. A row this build did
/// not write — another device, a newer build, a hand edit — must never put
/// a duplicate chip or an empty one on screen.
pub fn contexts_from_stored(raw: &str) -> Option<Vec<String>> {
    let values = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(values)) => values,
        _ => return None,
    };
    let mut names: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        let serde_json::Value::String(raw_name) = value else {
            return None;
        };
        let Ok(name) = normalise_context_name(&raw_name) else {
            continue;
        };
        if names.iter().any(|existing| same_context(existing, &name)) {
            continue;
        }
        names.push(name);
    }
    Some(names)
}

/// The build's default list as owned strings — what a device reads when
/// no `contexts` row exists yet.
pub fn default_contexts() -> Vec<String> {
    DEFAULT_CONTEXTS
        .iter()
        .map(|name| name.to_string())
        .collect()
}

/// Whether `candidate` names the same context as `name` — the module
/// header's matching rule.
pub fn same_context(name: &str, candidate: &str) -> bool {
    normalize_context(name) == normalize_context(candidate)
}

/// Why a name cannot be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextNameError {
    /// Nothing but whitespace.
    Empty,
    /// [`NO_CONTEXT`] — the frontier's own label for the *absence* of a
    /// context, which a chip already answers to.
    Reserved,
}

/// Trims a typed name and refuses the two shapes that can never be a
/// context. `@` is not required: the field is free text and an item may
/// already carry `errands`.
pub fn normalise_context_name(raw: &str) -> Result<String, ContextNameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ContextNameError::Empty);
    }
    if name.eq_ignore_ascii_case(NO_CONTEXT) {
        return Err(ContextNameError::Reserved);
    }
    Ok(name.to_string())
}

/// One suggested context as the Settings section reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextEntry {
    pub name: String,
    /// How many live items carry it (the matching rule above) — what a
    /// removal will clear, said before it is done.
    pub item_count: usize,
}

/// The list as a reader sees it. One row means one `pending` fact, not
/// one per entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestedContexts {
    pub entries: Vec<ContextEntry>,
    /// Whether an unconfirmed local write is currently overlaid on the
    /// row — [`crate::bindings::Binding::pending`] verbatim.
    pub pending: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_frozen() {
        assert_eq!(CONTEXTS_KEY, "contexts");
    }

    #[test]
    fn the_key_collides_with_no_binding_and_no_switch() {
        for key in crate::bindings::BindingKey::ALL {
            assert_ne!(key.as_str(), CONTEXTS_KEY);
        }
        assert!(!crate::question_switch::all_switch_keys().contains(CONTEXTS_KEY));
    }

    #[test]
    fn only_an_array_of_strings_reads_as_a_list() {
        assert_eq!(
            contexts_from_stored(r#"["@home","@calls"]"#),
            Some(vec!["@home".to_string(), "@calls".to_string()])
        );
        assert_eq!(contexts_from_stored("[]"), Some(vec![]));
        // Sanitised on the way in: blank, reserved and same-context
        // duplicates go, first spelling kept, order otherwise preserved.
        assert_eq!(
            contexts_from_stored(r#"[" @calls ", "", "no context", "@Calls", "calls", "@home"]"#),
            Some(vec!["@calls".to_string(), "@home".to_string()])
        );
        for raw in [
            "\"@home\"",
            "[1]",
            "[\"@a\", 2]",
            "false",
            "null",
            "{}",
            "",
            "not json",
        ] {
            assert_eq!(contexts_from_stored(raw), None, "{raw}");
        }
    }

    #[test]
    fn the_defaults_are_the_vocabulary_module_s_list() {
        assert_eq!(
            default_contexts(),
            [
                "@home",
                "@computer",
                "@phone",
                "@errands",
                "@garden",
                "@homework"
            ]
        );
    }

    #[test]
    fn a_name_is_trimmed_and_the_two_impossible_shapes_are_refused() {
        assert_eq!(
            normalise_context_name("  @calls "),
            Ok("@calls".to_string())
        );
        assert_eq!(normalise_context_name("calls"), Ok("calls".to_string()));
        assert_eq!(normalise_context_name("   "), Err(ContextNameError::Empty));
        assert_eq!(normalise_context_name(""), Err(ContextNameError::Empty));
        assert_eq!(
            normalise_context_name("no context"),
            Err(ContextNameError::Reserved)
        );
        assert_eq!(
            normalise_context_name(" No Context "),
            Err(ContextNameError::Reserved)
        );
    }

    #[test]
    fn matching_is_the_ranker_s_rule() {
        assert!(same_context("@errands", "@Errands"));
        assert!(same_context("@errands", "errands "));
        assert!(!same_context("@errands", "@errand"));
    }

    #[test]
    fn the_list_crosses_as_entries_and_one_pending_fact() {
        assert_eq!(
            serde_json::to_string(&SuggestedContexts {
                entries: vec![ContextEntry {
                    name: "@home".to_string(),
                    item_count: 2
                }],
                pending: true,
            })
            .unwrap(),
            r#"{"entries":[{"name":"@home","item_count":2}],"pending":true}"#
        );
    }
}
